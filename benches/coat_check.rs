#![feature(test)]

extern crate coatcheck;
extern crate test;

use test::Bencher;
use std::collections::HashMap;

use coatcheck::*;


#[bench]
fn bench_hash_map(b: &mut Bencher) {
    let mut map = HashMap::with_capacity(6);
    b.iter(|| {
        map.insert(0us, "something");
        map.insert(1, "something");
        let _ = map.remove(&0);
        map.insert(2, "something");
        map.insert(3, "something");
        map.insert(4, "something");
        let _ = map.remove(&3);
        map.insert(5, "something");
        let _ = map.remove(&2);
        let _ = map.remove(&1);
        let _ = map.remove(&4);
        let _ = map.remove(&5);
    });
}

#[bench]
fn bench_coat_check(b: &mut Bencher) {
    let mut cc = CoatCheck::with_capacity(6);
    b.iter(|| {
        let t1 = cc.check("something");
        let t2 = cc.check("something");
        let _ = cc.claim(t1);
        let t3 = cc.check("something");
        let t4 = cc.check("something");
        let t5 = cc.check("something");
        let _ = cc.claim(t4);
        let t6 = cc.check("something");
        let _ = cc.claim(t3);
        let _ = cc.claim(t2);
        let _ = cc.claim(t5);
        let _ = cc.claim(t6);
    });
}

#[bench]
fn bench_box(b: &mut Bencher) {
    b.iter(|| {
        let b1 = Box::new("something");
        let b2 = Box::new("something");
        drop(b1);
        let b3 = Box::new("something");
        let b4 = Box::new("something");
        let b5 = Box::new("something");
        drop(b4);
        let b6 = Box::new("something");
        drop(b3);
        drop(b2);
        drop(b5);
        drop(b6);
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

