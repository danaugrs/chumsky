#![feature(test, array_methods)]

extern crate test;

use test::{black_box, Bencher};

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Str(String),
    Num(f64),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsonZero<'a> {
    Null,
    Bool(bool),
    Str(&'a [u8]),
    Num(f64),
    Array(Vec<JsonZero<'a>>),
    Object(Vec<(&'a [u8], JsonZero<'a>)>),
}

static JSON: &'static [u8] = include_bytes!("sample.json");

#[bench]
fn chumsky_zero_copy(b: &mut Bencher) {
    use ::chumsky::input::*;

    let json = chumsky_zero_copy::json();
    b.iter(|| black_box(json.parse(JSON).unwrap()));
}

#[bench]
fn chumsky_zero_copy_check(b: &mut Bencher) {
    use ::chumsky::input::*;

    let json = chumsky_zero_copy::json();
    b.iter(|| black_box(json.check(JSON).unwrap()));
}

#[bench]
fn chumsky(b: &mut Bencher) {
    use ::chumsky::prelude::*;

    let json = chumsky::json();
    b.iter(|| black_box(json.parse(JSON).unwrap()));
}

#[bench]
fn serde_json(b: &mut Bencher) {
    use serde_json::{from_slice, Value};

    b.iter(|| black_box(from_slice::<Value>(JSON).unwrap()));
}

#[bench]
fn pom(b: &mut Bencher) {
    let json = pom::json();
    b.iter(|| black_box(json.parse(JSON).unwrap()));
}

mod chumsky_zero_copy {
    use chumsky::input::*;

    use super::JsonZero;
    use std::str;

    pub fn json<'a>() -> impl Parser<'a, [u8], Output = JsonZero<'a>> {
        recursive(|value| {
            let digits = filter(|b: &u8| b.is_ascii_digit())
                .repeated()
                .at_least(1);

            let frac = just(b'.').then(digits.clone());

            let exp = just(b'e').or(just(b'E'))
                .then(just(b'+').or(just(b'-')).or_not())
                .then(digits.clone());

            let number = just(b'-')
                .or_not()
                .then(digits/*text::int(10)*/)
                .then(frac.or_not())
                .then(exp.or_not())
                .map_slice(|bytes| {
                    str::from_utf8(bytes).unwrap().parse()
                        .map_err(|e| {
                            println!("Float: \"{}\"", str::from_utf8(bytes).unwrap());
                            e
                        })
                        .unwrap()
                });

            let escape = just(b'\\').then(choice((
                just(b'\\'),
                just(b'/'),
                just(b'"'),
                just(b'b').to(b'\x08'),
                just(b'f').to(b'\x0C'),
                just(b'n').to(b'\n'),
                just(b'r').to(b'\r'),
                just(b't').to(b'\t'),
            )))
                .ignored();

            let string = filter(|c| *c != b'\\' && *c != b'"').ignored().or(escape)
                .repeated()
                .map_slice(|bytes| bytes)
                .delimited_by(just(b'"'), just(b'"'));
                // .boxed();

            let array = value
                .clone()
                // .separated_by(just(b',').padded())
                    .then(just(b',').padded())
                    .map(|(x, _)| x)
                    .repeated()
                    .collect::<Vec<_>>()
                    .then(value.clone().or_not())
                    .map(|(mut xs, x)| {
                        if let Some(x) = x { xs.push(x); }
                        xs
                    })
                .padded()
                .delimited_by(just(b'['), just(b']'))
                .map(JsonZero::Array);

            let member = string.clone().then(just(b':').padded()).map(|(s, _)| s).then(value);
            let object = member.clone()
                //.separated_by(just(b',').padded())
                    .then(just(b',').padded())
                    .map(|(x, _)| x)
                    .repeated()
                    .collect::<Vec<_>>()
                    .then(member.clone().or_not())
                    .map(|(mut xs, x)| {
                        if let Some(x) = x { xs.push(x); }
                        xs
                    })
                .padded()
                .delimited_by(just(b'{'), just(b'}'))
                // .collect::<Vec<(_, JsonZero)>>()
                .map(JsonZero::Object);

            choice((
                just(b"null").to(JsonZero::Null),
                just(b"true").to(JsonZero::Bool(true)),
                just(b"false").to(JsonZero::Bool(false)),
                number.map(JsonZero::Num),
                string.map(JsonZero::Str),
                array,
                object,
            ))
            .padded()
        })
        .then(end())
        .map(|(json, _)| json)
    }
}

mod chumsky {
    use chumsky::{error::Cheap, prelude::*};

    use super::Json;
    use std::str;

    pub fn json() -> impl Parser<u8, Json, Error = Cheap<u8>> {
        recursive(|value| {
            let frac = just(b'.').chain(text::digits(10));

            let exp = one_of(b"eE")
                .ignore_then(just(b'+').or(just(b'-')).or_not())
                .chain(text::digits(10));

            let number = just(b'-')
                .or_not()
                .chain(text::int(10))
                .chain(frac.or_not().flatten())
                .chain::<u8, _, _>(exp.or_not().flatten())
                .map(|bytes| str::from_utf8(&bytes.as_slice()).unwrap().parse().unwrap());

            let escape = just(b'\\').ignore_then(choice((
                just(b'\\'),
                just(b'/'),
                just(b'"'),
                just(b'b').to(b'\x08'),
                just(b'f').to(b'\x0C'),
                just(b'n').to(b'\n'),
                just(b'r').to(b'\r'),
                just(b't').to(b'\t'),
            )));

            let string = just(b'"')
                .ignore_then(filter(|c| *c != b'\\' && *c != b'"').or(escape).repeated())
                .then_ignore(just(b'"'))
                .map(|bytes| String::from_utf8(bytes).unwrap());

            let array = value
                .clone()
                .separated_by(just(b',').padded())
                .padded()
                .delimited_by(just(b'['), just(b']'))
                .map(Json::Array);

            let member = string.then_ignore(just(b':').padded()).then(value);
            let object = member
                .separated_by(just(b',').padded())
                .padded()
                .delimited_by(just(b'{'), just(b'}'))
                .collect::<Vec<(String, Json)>>()
                .map(Json::Object);

            choice((
                just(b"null").to(Json::Null),
                just(b"true").to(Json::Bool(true)),
                just(b"false").to(Json::Bool(false)),
                number.map(Json::Num),
                string.map(Json::Str),
                array,
                object,
            ))
            .padded()
        })
        .then_ignore(end())
    }
}

mod pom {
    use pom::parser::*;
    use pom::Parser;

    use super::Json;
    use std::str::{self, FromStr};

    fn space() -> Parser<u8, ()> {
        one_of(b" \t\r\n").repeat(0..).discard()
    }

    fn number() -> Parser<u8, f64> {
        let integer = one_of(b"123456789") - one_of(b"0123456789").repeat(0..) | sym(b'0');
        let frac = sym(b'.') + one_of(b"0123456789").repeat(1..);
        let exp = one_of(b"eE") + one_of(b"+-").opt() + one_of(b"0123456789").repeat(1..);
        let number = sym(b'-').opt() + integer + frac.opt() + exp.opt();
        number
            .collect()
            .convert(str::from_utf8)
            .convert(|s| f64::from_str(&s))
    }

    fn string() -> Parser<u8, String> {
        let special_char = sym(b'\\')
            | sym(b'/')
            | sym(b'"')
            | sym(b'b').map(|_| b'\x08')
            | sym(b'f').map(|_| b'\x0C')
            | sym(b'n').map(|_| b'\n')
            | sym(b'r').map(|_| b'\r')
            | sym(b't').map(|_| b'\t');
        let escape_sequence = sym(b'\\') * special_char;
        let string = sym(b'"') * (none_of(b"\\\"") | escape_sequence).repeat(0..) - sym(b'"');
        string.convert(String::from_utf8)
    }

    fn array() -> Parser<u8, Vec<Json>> {
        let elems = list(call(value), sym(b',') * space());
        sym(b'[') * space() * elems - sym(b']')
    }

    fn object() -> Parser<u8, Vec<(String, Json)>> {
        let member = string() - space() - sym(b':') - space() + call(value);
        let members = list(member, sym(b',') * space());
        let obj = sym(b'{') * space() * members - sym(b'}');
        obj.map(|members| members.into_iter().collect::<Vec<_>>())
    }

    fn value() -> Parser<u8, Json> {
        (seq(b"null").map(|_| Json::Null)
            | seq(b"true").map(|_| Json::Bool(true))
            | seq(b"false").map(|_| Json::Bool(false))
            | number().map(|num| Json::Num(num))
            | string().map(|text| Json::Str(text))
            | array().map(|arr| Json::Array(arr))
            | object().map(|obj| Json::Object(obj)))
            - space()
    }

    pub fn json() -> Parser<u8, Json> {
        space() * value() - end()
    }
}
