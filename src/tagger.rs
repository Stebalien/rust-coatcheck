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
    NEXT_LOCAL_TAG.with(|tag| match tag.get() {
        Tag { offset: u16::MAX, .. } => {
            let prefix = GLOBAL_TAG_PREFIX.next();
            tag.set(Tag { prefix: prefix, offset: 1 });
            Tag { prefix: prefix, offset: 0 }
        }, 
        inner_tag => {
            tag.set(Tag {
                prefix: inner_tag.prefix,
                offset: inner_tag.offset + 1
            });
            inner_tag
        }
    })
}
