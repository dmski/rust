// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Basic html highlighting functionality
//!
//! This module uses libsyntax's lexer to provide token-based highlighting for
//! the HTML documentation generated by rustdoc.

use std::str;
use std::io;

use syntax::parse;
use syntax::parse::lexer;
use syntax::codemap::{BytePos, Span};

use html::escape::Escape;

use t = syntax::parse::token;

/// Highlights some source code, returning the HTML output.
pub fn highlight(src: &str, class: Option<&str>) -> ~str {
    let sess = parse::new_parse_sess();
    let fm = parse::string_to_filemap(&sess, src.to_owned(), ~"<stdin>");

    let mut out = io::MemWriter::new();
    doit(&sess,
         lexer::new_string_reader(&sess.span_diagnostic, fm),
         class,
         &mut out).unwrap();
    str::from_utf8_lossy(out.unwrap()).into_owned()
}

/// Exhausts the `lexer` writing the output into `out`.
///
/// The general structure for this method is to iterate over each token,
/// possibly giving it an HTML span with a class specifying what flavor of token
/// it's used. All source code emission is done as slices from the source map,
/// not from the tokens themselves, in order to stay true to the original
/// source.
fn doit(sess: &parse::ParseSess, lexer: lexer::StringReader, class: Option<&str>,
        out: &mut Writer) -> io::IoResult<()> {
    use syntax::parse::lexer::Reader;

    try!(write!(out, "<pre class='rust {}'>\n", class.unwrap_or("")));
    let mut last = BytePos(0);
    let mut is_attribute = false;
    let mut is_macro = false;
    let mut is_macro_nonterminal = false;
    loop {
        let next = lexer.next_token();
        let test = if next.tok == t::EOF {lexer.pos.get()} else {next.sp.lo};

        // The lexer consumes all whitespace and non-doc-comments when iterating
        // between tokens. If this token isn't directly adjacent to our last
        // token, then we need to emit the whitespace/comment.
        //
        // If the gap has any '/' characters then we consider the whole thing a
        // comment. This will classify some whitespace as a comment, but that
        // doesn't matter too much for syntax highlighting purposes.
        if test > last {
            let snip = sess.span_diagnostic.cm.span_to_snippet(Span {
                lo: last,
                hi: test,
                expn_info: None,
            }).unwrap();
            if snip.contains("/") {
                try!(write!(out, "<span class='comment'>{}</span>",
                              Escape(snip)));
            } else {
                try!(write!(out, "{}", Escape(snip)));
            }
        }
        last = next.sp.hi;
        if next.tok == t::EOF { break }

        let klass = match next.tok {
            // If this '&' token is directly adjacent to another token, assume
            // that it's the address-of operator instead of the and-operator.
            // This allows us to give all pointers their own class (~ and @ are
            // below).
            t::BINOP(t::AND) if lexer.peek().sp.lo == next.sp.hi => "kw-2",
            t::AT | t::TILDE => "kw-2",

            // consider this as part of a macro invocation if there was a
            // leading identifier
            t::NOT if is_macro => { is_macro = false; "macro" }

            // operators
            t::EQ | t::LT | t::LE | t::EQEQ | t::NE | t::GE | t::GT |
                t::ANDAND | t::OROR | t::NOT | t::BINOP(..) | t::RARROW |
                t::BINOPEQ(..) | t::FAT_ARROW => "op",

            // miscellaneous, no highlighting
            t::DOT | t::DOTDOT | t::DOTDOTDOT | t::COMMA | t::SEMI |
                t::COLON | t::MOD_SEP | t::LARROW | t::DARROW | t::LPAREN |
                t::RPAREN | t::LBRACKET | t::LBRACE | t::RBRACE => "",
            t::DOLLAR => {
                if t::is_ident(&lexer.peek().tok) {
                    is_macro_nonterminal = true;
                    "macro-nonterminal"
                } else {
                    ""
                }
            }

            // This is the start of an attribute. We're going to want to
            // continue highlighting it as an attribute until the ending ']' is
            // seen, so skip out early. Down below we terminate the attribute
            // span when we see the ']'.
            t::POUND => {
                is_attribute = true;
                try!(write!(out, r"<span class='attribute'>\#"));
                continue
            }
            t::RBRACKET => {
                if is_attribute {
                    is_attribute = false;
                    try!(write!(out, "]</span>"));
                    continue
                } else {
                    ""
                }
            }

            // text literals
            t::LIT_CHAR(..) | t::LIT_STR(..) | t::LIT_STR_RAW(..) => "string",

            // number literals
            t::LIT_INT(..) | t::LIT_UINT(..) | t::LIT_INT_UNSUFFIXED(..) |
                t::LIT_FLOAT(..) | t::LIT_FLOAT_UNSUFFIXED(..) => "number",

            // keywords are also included in the identifier set
            t::IDENT(ident, _is_mod_sep) => {
                match t::get_ident(ident).get() {
                    "ref" | "mut" => "kw-2",

                    "self" => "self",
                    "false" | "true" => "boolval",

                    "Option" | "Result" => "prelude-ty",
                    "Some" | "None" | "Ok" | "Err" => "prelude-val",

                    _ if t::is_any_keyword(&next.tok) => "kw",
                    _ => {
                        if is_macro_nonterminal {
                            is_macro_nonterminal = false;
                            "macro-nonterminal"
                        } else if lexer.peek().tok == t::NOT {
                            is_macro = true;
                            "macro"
                        } else {
                            "ident"
                        }
                    }
                }
            }

            t::LIFETIME(..) => "lifetime",
            t::DOC_COMMENT(..) => "doccomment",
            t::UNDERSCORE | t::EOF | t::INTERPOLATED(..) => "",
        };

        // as mentioned above, use the original source code instead of
        // stringifying this token
        let snip = sess.span_diagnostic.cm.span_to_snippet(next.sp).unwrap();
        if klass == "" {
            try!(write!(out, "{}", Escape(snip)));
        } else {
            try!(write!(out, "<span class='{}'>{}</span>", klass,
                          Escape(snip)));
        }
    }

    write!(out, "</pre>\n")
}
