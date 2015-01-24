#![allow(unstable)]
#![feature(box_syntax)]

extern crate coatcheck;
extern crate test;

use test::Bencher;
use std::collections::HashMap;

use coatcheck::*;


#[bench]
fn bench_hash_map(b: &mut Bencher) {
    b.iter(|| {
        let mut map = HashMap::with_capacity(0);
        let mut res = Vec::with_capacity(20);
        for i in 0..20 {
            map.insert(i, "something");
            res.push(i);
        }
        for i in res.into_iter() {
            map.remove(&i);
        }
    });
}

#[bench]
fn bench_coat_check(b: &mut Bencher) {
    b.iter(|| {
        let mut cc = CoatCheck::with_capacity(0);
        let mut res = Vec::with_capacity(20);
        for _ in 0..20 {
            res.push(cc.check("something"));
        }
        for t in res.into_iter() {
            let _ = test::black_box(cc.claim(t));
        }
    });
}

#[bench]
fn bench_box(b: &mut Bencher) {
    b.iter(|| {
        let mut res = Vec::with_capacity(20);
        for _ in 0..20 {
            res.push(Box::new("something"));
        }
        for t in res.into_iter() {
            let box item = t;
            test::black_box(item);
        }
    });
}

#[bench]
fn bench_coat_check_init(b: &mut Bencher) {
    b.iter(|| {
        for _ in 0..1000 {
            test::black_box(CoatCheck::<u64>::new());
        }
    });
}

#[bench]
fn bench_coat_check_access(b: &mut Bencher) {
    let mut cc = CoatCheck::new();
    let mut tickets = Vec::with_capacity(100);
    for _ in 0..100 {
        tickets.push(cc.check("something"));
    }
    let ref t = tickets[20];
    b.iter(|&: | {
        test::black_box(&cc[*t]);
    });
}

#[bench]
fn bench_hash_map_access(b: &mut Bencher) {
    let mut map = HashMap::with_capacity(100);
    for i in 0i32..100 {
        map.insert(i, "something");
    }
    b.iter(|&:| {
        test::black_box(&map[20i32]);
    });
}

