#![feature(test)]

extern crate coatcheck;
extern crate test;

use coatcheck::*;

#[test]
fn two_cc() {
    let mut c1 = CoatCheck::new();
    let mut c2 = CoatCheck::new();

    let t1 = c1.check(1);
    let t2 = c1.check(2);
    assert_eq!(c1[&t1], 1);
    assert_eq!(c1[&t2], 2);
    assert_eq!(c1[&t1], 1);
    assert_eq!(c1.claim(t1).unwrap(), 1);
    let t3 = c1.check(3);
    assert_eq!(c1.claim(t3).unwrap(), 3);

    let t4 = c2.check(4);
    let _ = c2.check(5);

    assert!(c2.claim(t2).is_err());
    assert!(c1.claim(t4).is_err());
}

#[test]
fn iter() {
    let mut cc = CoatCheck::new();
    cc.check_all(0..2).count();
    {
        let mut iter = cc.iter();
        assert_eq!(iter.next(), Some(&0));
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next(), None);
    }

    {
        let mut iter = cc.iter_mut();
        assert_eq!(iter.next(), Some(&mut 0));
        let it = iter.next().unwrap();
        assert_eq!(it, &mut 1);
        *it = 2;
        assert_eq!(iter.next(), None);
    }

    {
        let mut iter = cc.into_iter();
        assert_eq!(iter.next(), Some(0));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), None)
    }
}

#[test]
fn get() {
    let mut cc = CoatCheck::new();
    let tickets: Vec<Ticket> = cc.check_all(0usize..10).collect();
    for (i, t) in tickets.iter().enumerate() {
        assert_eq!(cc[t], i);
    }
    cc[&tickets[2]] = 1;
    assert_eq!(cc[&tickets[2]], 1);
}

#[test]
fn check_all() {
    let mut cc = CoatCheck::new();
    let mut v: Vec<Ticket> = cc.check_all(vec![1i32,2,3,4].into_iter()).collect();
    assert_eq!(cc.len(), 4);
    assert_eq!(cc.claim(v.pop().unwrap()).unwrap(), 4);
    assert_eq!(cc.claim(v.pop().unwrap()).unwrap(), 3);
    assert_eq!(cc.claim(v.pop().unwrap()).unwrap(), 2);
    assert_eq!(cc.claim(v.pop().unwrap()).unwrap(), 1);
    assert!(v.is_empty());
    assert!(cc.is_empty());
}
