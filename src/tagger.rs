//! An efficient module for generating unique IDs
//! The unique ID's are 128bits so you can theoretically run out of them but that's very unlikely.
use std::cell::{UnsafeCell, Cell};
use std::sync::{StaticMutex, MUTEX_INIT};
use std::marker::Sync;
use std::u16;
use std::num::Int;


#[derive(Copy, PartialEq, Eq)]
pub struct Tag {
    prefix: TagPrefix,
    offset: u16,
}
#[derive(Copy, PartialEq, Eq)]
struct TagPrefix(u64, u32, u16);

struct Tagger {
    mutex: StaticMutex,
    value: UnsafeCell<(u64, u64)>,
}

impl Tagger {
    fn next(&'static self) -> TagPrefix {
        let old;
        unsafe {
            let _l = self.mutex.lock().unwrap();
            old = *self.value.get();
            *self.value.get() = match old.1 + 1 {
                n if n <= (u16::MAX as u64) => (old.0, n),
                _ => match old.0.checked_add(1) {
                    Some(n) => (n, 0),
                    None => panic!("CoatCheck ID overflow!")
                }
            };
        }
        TagPrefix(old.0, (old.1 >> 16) as u32, old.1 as u16)
    }
}

unsafe impl Sync for Tagger {}

static GLOBAL_TAG_PREFIX: Tagger = Tagger {
    mutex: MUTEX_INIT,
    value: UnsafeCell { value: (0, 0) },
};

thread_local!(static NEXT_LOCAL_TAG: Cell<Tag> = Cell::new(Tag { prefix: GLOBAL_TAG_PREFIX.next(), offset: 0 }));

#[inline]
pub fn next_tag() -> Tag {
    NEXT_LOCAL_TAG.with(|tag| {
        let next_tag = tag.get();
        tag.set(if next_tag.offset == u16::MAX {
            Tag {
                prefix: GLOBAL_TAG_PREFIX.next(),
                offset: 0,
            }
        } else {
            Tag {
                prefix: next_tag.prefix,
                offset: next_tag.offset + 1
            }
        });
        next_tag
    })
}

#[test]
fn test_tagger_unthreaded() {
    let first_tag = next_tag();
    for i in first_tag.offset..(u16::MAX) {
        assert!(next_tag() == Tag { prefix: first_tag.prefix, offset: i+1});
    }
    let next = next_tag();
    assert!(next.prefix != first_tag.prefix);
    assert!(next.offset == 0);
    assert!(next_tag() == Tag { prefix: next.prefix, offset: 1 });
}

#[test]
fn test_tagger_threaded() {
    use std::sync::Future;
    use std::cmp::Ordering;
    let futures: Vec<Future<TagPrefix>> = (0..10).map(|_| {
        Future::spawn(move || {
            let tag = next_tag();
            assert_eq!(tag.offset, 0);
            tag.prefix
        })
    }).collect();
    let mut results: Vec<TagPrefix> = futures.into_iter().map(|x| x.into_inner()).collect();
    results.sort_by(|a, b| {
        match a.0.cmp(&b.0) {
            Ordering::Equal => match a.1.cmp(&b.1) {
                Ordering::Equal => a.2.cmp(&b.2),
                v => v,
            },
            v => v,
        }
    });
    let old_len = results.len();
    results.dedup();
    assert_eq!(old_len, results.len());
}
