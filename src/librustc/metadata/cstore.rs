// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(non_camel_case_types)];

// The crate store - a central repo for information collected about external
// crates and libraries

use back::svh::Svh;
use metadata::decoder;
use metadata::loader;

use std::cell::RefCell;
use std::c_vec::CVec;
use collections::HashMap;
use syntax::ast;
use syntax::parse::token::IdentInterner;
use syntax::crateid::CrateId;

// A map from external crate numbers (as decoded from some crate file) to
// local crate numbers (as generated during this session). Each external
// crate may refer to types in other external crates, and each has their
// own crate numbers.
pub type cnum_map = @RefCell<HashMap<ast::CrateNum, ast::CrateNum>>;

pub enum MetadataBlob {
    MetadataVec(CVec<u8>),
    MetadataArchive(loader::ArchiveMetadata),
}

pub struct crate_metadata {
    name: ~str,
    data: MetadataBlob,
    cnum_map: cnum_map,
    cnum: ast::CrateNum
}

#[deriving(Eq)]
pub enum LinkagePreference {
    RequireDynamic,
    RequireStatic,
}

#[deriving(Eq, FromPrimitive)]
pub enum NativeLibaryKind {
    NativeStatic,    // native static library (.a archive)
    NativeFramework, // OSX-specific
    NativeUnknown,   // default way to specify a dynamic library
}

// Where a crate came from on the local filesystem. One of these two options
// must be non-None.
#[deriving(Eq, Clone)]
pub struct CrateSource {
    dylib: Option<Path>,
    rlib: Option<Path>,
    cnum: ast::CrateNum,
}

pub struct CStore {
    priv metas: RefCell<HashMap<ast::CrateNum, @crate_metadata>>,
    priv extern_mod_crate_map: RefCell<extern_mod_crate_map>,
    priv used_crate_sources: RefCell<Vec<CrateSource> >,
    priv used_libraries: RefCell<Vec<(~str, NativeLibaryKind)> >,
    priv used_link_args: RefCell<Vec<~str> >,
    intr: @IdentInterner
}

// Map from NodeId's of local extern crate statements to crate numbers
type extern_mod_crate_map = HashMap<ast::NodeId, ast::CrateNum>;

impl CStore {
    pub fn new(intr: @IdentInterner) -> CStore {
        CStore {
            metas: RefCell::new(HashMap::new()),
            extern_mod_crate_map: RefCell::new(HashMap::new()),
            used_crate_sources: RefCell::new(Vec::new()),
            used_libraries: RefCell::new(Vec::new()),
            used_link_args: RefCell::new(Vec::new()),
            intr: intr
        }
    }

    pub fn get_crate_data(&self, cnum: ast::CrateNum) -> @crate_metadata {
        let metas = self.metas.borrow();
        *metas.get().get(&cnum)
    }

    pub fn get_crate_hash(&self, cnum: ast::CrateNum) -> Svh {
        let cdata = self.get_crate_data(cnum);
        decoder::get_crate_hash(cdata.data())
    }

    pub fn get_crate_id(&self, cnum: ast::CrateNum) -> CrateId {
        let cdata = self.get_crate_data(cnum);
        decoder::get_crate_id(cdata.data())
    }

    pub fn set_crate_data(&self, cnum: ast::CrateNum, data: @crate_metadata) {
        let mut metas = self.metas.borrow_mut();
        metas.get().insert(cnum, data);
    }

    pub fn have_crate_data(&self, cnum: ast::CrateNum) -> bool {
        let metas = self.metas.borrow();
        metas.get().contains_key(&cnum)
    }

    pub fn iter_crate_data(&self, i: |ast::CrateNum, @crate_metadata|) {
        let metas = self.metas.borrow();
        for (&k, &v) in metas.get().iter() {
            i(k, v);
        }
    }

    pub fn add_used_crate_source(&self, src: CrateSource) {
        let mut used_crate_sources = self.used_crate_sources.borrow_mut();
        if !used_crate_sources.get().contains(&src) {
            used_crate_sources.get().push(src);
        }
    }

    pub fn get_used_crate_source(&self, cnum: ast::CrateNum)
                                     -> Option<CrateSource> {
        let mut used_crate_sources = self.used_crate_sources.borrow_mut();
        used_crate_sources.get().iter().find(|source| source.cnum == cnum)
            .map(|source| source.clone())
    }

    pub fn reset(&self) {
        self.metas.with_mut(|s| s.clear());
        self.extern_mod_crate_map.with_mut(|s| s.clear());
        self.used_crate_sources.with_mut(|s| s.clear());
        self.used_libraries.with_mut(|s| s.clear());
        self.used_link_args.with_mut(|s| s.clear());
    }

    // This method is used when generating the command line to pass through to
    // system linker. The linker expects undefined symbols on the left of the
    // command line to be defined in libraries on the right, not the other way
    // around. For more info, see some comments in the add_used_library function
    // below.
    //
    // In order to get this left-to-right dependency ordering, we perform a
    // topological sort of all crates putting the leaves at the right-most
    // positions.
    pub fn get_used_crates(&self, prefer: LinkagePreference)
                           -> Vec<(ast::CrateNum, Option<Path>)> {
        let mut ordering = Vec::new();
        fn visit(cstore: &CStore, cnum: ast::CrateNum,
                 ordering: &mut Vec<ast::CrateNum>) {
            if ordering.as_slice().contains(&cnum) { return }
            let meta = cstore.get_crate_data(cnum);
            for (_, &dep) in meta.cnum_map.borrow().get().iter() {
                visit(cstore, dep, ordering);
            }
            ordering.push(cnum);
        };
        for (&num, _) in self.metas.borrow().get().iter() {
            visit(self, num, &mut ordering);
        }
        ordering.as_mut_slice().reverse();
        let ordering = ordering.as_slice();
        let used_crate_sources = self.used_crate_sources.borrow();
        let mut libs = used_crate_sources.get()
            .iter()
            .map(|src| (src.cnum, match prefer {
                RequireDynamic => src.dylib.clone(),
                RequireStatic => src.rlib.clone(),
            }))
            .collect::<Vec<(ast::CrateNum, Option<Path>)>>();
        libs.sort_by(|&(a, _), &(b, _)| {
            ordering.position_elem(&a).cmp(&ordering.position_elem(&b))
        });
        libs
    }

    pub fn add_used_library(&self, lib: ~str, kind: NativeLibaryKind) {
        assert!(!lib.is_empty());
        let mut used_libraries = self.used_libraries.borrow_mut();
        used_libraries.get().push((lib, kind));
    }

    pub fn get_used_libraries<'a>(&'a self)
                              -> &'a RefCell<Vec<(~str, NativeLibaryKind)> > {
        &self.used_libraries
    }

    pub fn add_used_link_args(&self, args: &str) {
        let mut used_link_args = self.used_link_args.borrow_mut();
        for s in args.split(' ') {
            used_link_args.get().push(s.to_owned());
        }
    }

    pub fn get_used_link_args<'a>(&'a self) -> &'a RefCell<Vec<~str> > {
        &self.used_link_args
    }

    pub fn add_extern_mod_stmt_cnum(&self,
                                    emod_id: ast::NodeId,
                                    cnum: ast::CrateNum) {
        let mut extern_mod_crate_map = self.extern_mod_crate_map.borrow_mut();
        extern_mod_crate_map.get().insert(emod_id, cnum);
    }

    pub fn find_extern_mod_stmt_cnum(&self, emod_id: ast::NodeId)
                                     -> Option<ast::CrateNum> {
        let extern_mod_crate_map = self.extern_mod_crate_map.borrow();
        extern_mod_crate_map.get().find(&emod_id).map(|x| *x)
    }
}

impl crate_metadata {
    pub fn data<'a>(&'a self) -> &'a [u8] { self.data.as_slice() }
}

impl MetadataBlob {
    pub fn as_slice<'a>(&'a self) -> &'a [u8] {
        match *self {
            MetadataVec(ref vec) => vec.as_slice(),
            MetadataArchive(ref ar) => ar.as_slice(),
        }
    }
}
