// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

trait A<T> {}
struct B<'a, T>(&'a A<T>);

trait X<'a> {}
impl<'a, T> X<'a> for B<'a, T> {}

fn f<'a, T, U>(v: &'a A<T>) -> ~X<'a>: {
    ~B(v) as ~X<'a>: //~ ERROR value may contain references; add `'static` bound to `T`
}

fn g<'a, T, U>(v: &'a A<U>) -> ~X<'a>: {
    ~B(v) as ~X<'a>: //~ ERROR value may contain references; add `'static` bound to `U`
}

fn h<'a, T: 'static>(v: &'a A<T>) -> ~X<'a>: {
    ~B(v) as ~X<'a>: // ok
}

fn main() {}

