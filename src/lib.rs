//! # CoatCheck
//!
//! CoatCheck is a library for storing values and referencing them by "handles" (`Tickets`). This
//! library is designed to be used where you would otherwise use a hash table but you don't
//! actually need to be able to choose the keys.
//!
//! ## Advantages over a hash table:
//!
//! 1. You don't have to generate your keys.
//! 2. CoatCheck is at least 5x faster than the stdlib HashTable for insert/remove operations.
//! 3. CoatCheck is about 40x faster for lookup operations.
//!
//! ## Example
//!
//! For example, let's say you were implementing a callback system:
//!
//! ```rust
//! struct System {
//!     callbacks: Vec<Box<FnMut() + 'static>>,
//! }
//!
//! impl System {
//!     fn add_callback<C>(&mut self, cb: C) where C: FnMut() + 'static {
//!         self.callbacks.push(Box::new(cb));
//!     }
//!     fn fire(&mut self) {
//!         for cb in self.callbacks.iter_mut() {
//!             (cb)();
//!         }
//!     }
//! }
//! ```
//!
//! This system works but doesn't allow unregistering. If you wanted to allow
//! unregistering individual callbacks, you could do something like:
//!
//! ```rust
//! use std::collections::HashMap;
//!
//! struct System {
//!     callbacks: HashMap<usize, Box<FnMut() + 'static>>,
//!     next_id: usize,
//! }
//!
//! struct Handle {
//!     id: usize,
//! }
//!
//! impl System {
//!     fn add_callback<C>(&mut self, cb: C) -> Handle where C: FnMut() + 'static {
//!         let id = self.next_id;
//!         self.next_id += 1;
//!         self.callbacks.insert(id, Box::new(cb));
//!         Handle { id: id }
//!     }
//!     fn fire(&mut self) {
//!         for (_, cb) in self.callbacks.iter_mut() {
//!             (cb)();
//!         }
//!     }
//!     fn remove_callback(&mut self, handle: Handle) {
//!         self.callbacks.remove(&handle.id);
//!     }
//! }
//! ```
//!
//! However, we don't REALLY need a hash table because we don't care about the keys.
//!
//! This is where this library comes in. It acts like the above system *but* takes advantage of the
//! fact that it can choose the IDs:
//!
//! ```rust
//! use coatcheck::{CoatCheck, Ticket};
//!
//! struct System {
//!     callbacks: CoatCheck<Box<FnMut() + 'static>>,
//! }
//!
//! struct Handle {
//!     ticket: Ticket, // Wrap it for type safety
//! }
//!
//! impl System {
//!     fn add_callback<C>(&mut self, cb: C) -> Handle where C: FnMut() + 'static {
//!         Handle { ticket: self.callbacks.check(Box::new(cb)) }
//!     }
//!     fn remove_callback(&mut self, handle: Handle) {
//!         self.callbacks.claim(handle.ticket);
//!     }
//!     fn fire(&mut self) {
//!         for cb in self.callbacks.iter_mut() {
//!             (*cb)();
//!         }
//!     }
//! }
//! ```
//!
//! ## Discussion
//!
//! One thing you might note when using this library is that Tickets can't be duplicated in any way.
//!
//! ### Pros:
//!
//!  * Ownership: Preventing duplication of the ticket preserves ownership of the
//!    value to an extent. The value can still be stolen by destroying the
//!    CoatCheck but that's the only way to get it out without the ticket.
//!
//!  * Safety: As long as you use the ticket in the right coat check, the index
//!    operator will never panic.
//!  
//!  * Size: If I allowed ticket copying, I'd need to store a "generation" in every ticket and
//!    along side the ticket's associated value to be able to distinguish between an old ticket and
//!    a new one. Currently, I can get away with reusing tickets because they must be turned in
//!    before freeing a slot.
//!
//! ### Cons:
//!
//!  * Multiple references: There's no way to give away a reference to a value
//!    (without using actual references, that is).

#![allow(unstable)]
use std::rand::random;
use std::fmt;
use std::vec;
use std::ops::{Index, IndexMut};
use std::default::Default;
use std::slice;
use std::iter;
use std::num::Int;
use std::mem;
use std::iter::RandomAccessIterator;
use std::error;
use std::error::Error as ErrorTrait;
use Entry::*;

enum Entry<V> {
    Empty(usize /* next free index */),
    Full(V),
}

impl<V> Entry<V> {

    /// Take the value if it exists.
    fn full(self) -> Option<V> {
        match self {
            Full(value) => Some(value),
            Empty(_) => None
        }
    }

    /// Get an optional reference to the value.
    fn full_ref(&self) -> Option<&V> {
        match self {
            &Full(ref value) => Some(value),
            _ => None
        }
    }

    /// Get an optional mutable reference to the value.
    fn full_mut(&mut self) -> Option<&mut V> {
        match self {
            &mut Full(ref mut value) => Some(value),
            _ => None
        }
    }

    /// Is the entry full
    fn is_full(&self) -> bool {
        match self {
            &Full(_) => true,
            _ => false,
        }
    }

    /// Fill an empty entry with a value and return the next free index.
    fn fill(&mut self, value: V) -> usize {
        let mut other = Full(value);
        mem::swap(self, &mut other);
        match other {
            Empty(next_free) => next_free,
            _ => panic!("expected no entry"),
        }
    }

    /// Empty a full entry with one setting the next free index and returning the value.
    fn empty(&mut self, next_free: usize) -> V {
        let mut other = Empty(next_free);
        mem::swap(self, &mut other);
        match other {
            Full(value) => value,
            _ => panic!("expected an entry"),
        }
    }
}

/// A `Ticket` is an opaque data structure that can be used to claim the associated value.
///
/// *Note:* Tickets can't be copied to prevent re-use (a ticket can only be exchanged for exactly one
/// item).
#[allow(missing_copy_implementations)]
#[must_use = "you need this ticket to claim your item"]
pub struct Ticket {
    tag: usize,
    index: usize,
}

impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Ticket")
    }
}

/// Coat check error types
#[derive(Copy)]
pub enum ErrorKind {
    WrongCoatCheck,
}

impl ErrorKind {
    pub fn description(&self) -> &str {
        match self {
            &ErrorKind::WrongCoatCheck => "Ticket used in the wrong coat check"
        }
    }
}

/// The error yielded when a claim fails.
pub struct ClaimError {
    kind: ErrorKind,
    ticket: Ticket,
}

impl error::Error for ClaimError {
    fn description(&self) -> &str {
        self.kind.description()
    }
}

impl fmt::Display for ClaimError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ClaimError: {}", self.description())
    }
}

impl error::FromError<ClaimError> for Ticket {
    fn from_error(e: ClaimError) -> Ticket {
        e.ticket
    }
}

/// The error yielded an access fails.
#[derive(Copy)]
pub struct AccessError {
    kind: ErrorKind,
}

impl error::Error for AccessError {
    fn description(&self) -> &str {
        self.kind.description()
    }
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AccessError: {}", self.description())
    }
}

pub struct Tickets<'a, I, V> where I: Iterator<Item=V>, V: 'a {
    iter: I,
    cc: &'a mut CoatCheck<V>,
}

impl<'a, I, V> Iterator for Tickets<'a, I, V> where I: Iterator<Item=V>, V: 'a {
    type Item = Ticket;

    fn next(&mut self) -> Option<Ticket> {
        self.iter.next().map(|v| self.cc.check(v))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a, I, V> ExactSizeIterator for Tickets<'a, I, V> where I: ExactSizeIterator<Item=V>, V: 'a { }
impl<'a, I, V> DoubleEndedIterator for Tickets<'a, I, V> where I: DoubleEndedIterator<Item=V>, V: 'a {
    fn next_back(&mut self) -> Option<Ticket> {
        self.iter.next_back().map(|v| self.cc.check(v))
    }
}

impl<'a, I, V> RandomAccessIterator for Tickets<'a, I, V> where I: RandomAccessIterator<Item=V>, V: 'a {
    #[inline]
    fn indexable(&self) -> usize {
        self.iter.indexable()
    }

    #[inline]
    fn idx(&mut self, index: usize) -> Option<Ticket> {
        self.iter.idx(index).map(|v| self.cc.check(v))
    }

}

#[doc(hidden)]
struct GenericIter<V, I> where I: Iterator<Item=V> {
    inner: I,
    remaining: usize,

}

impl<V, I> ExactSizeIterator for GenericIter<V, I> where I: Iterator<Item=V> {
    #[inline]
    fn len(&self) -> usize {
        self.remaining
    }
}

impl<V, I> Iterator for GenericIter<V, I> where I: Iterator<Item = V> {
    type Item = V;
    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.inner.next()
        } else {
            None
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
}

#[doc(hidden)]
pub type IntoIter<V>    = GenericIter<V,
                                      iter::FilterMap<Entry<V>,
                                                      V,
                                                      vec::IntoIter<Entry<V>>,
                                      fn(Entry<V>) -> Option<V>>>;

#[doc(hidden)]
pub type Iter<'a, V>    = GenericIter<&'a V,
                                      iter::FilterMap<&'a Entry<V>,
                                                      &'a V,
                                                      slice::Iter<'a, Entry<V>>,
                                      fn(&'a Entry<V>) -> Option<&'a V>>>;

#[doc(hidden)]
pub type IterMut<'a, V> = GenericIter<&'a mut V,
                                      iter::FilterMap<&'a mut Entry<V>,
                                                      &'a mut V,
                                                      slice::IterMut<'a, Entry<V>>,
                                      fn(&'a mut Entry<V>) -> Option<&'a mut V>>>;

/// A data structure storing values indexed by tickets.
pub struct CoatCheck<V> {
    tag: usize,
    data: Vec<Entry<V>>,
    size: usize,
    next_free: usize,
}

impl<V> CoatCheck<V> {
    /// Constructs a new, empty `CoatCheck<T>`.
    ///
    /// The coat check will not allocate until elements are pushed onto it.
    ///
    /// # Examples
    ///
    /// ```
    /// use coatcheck::CoatCheck;
    ///
    /// let mut cc: CoatCheck<i32> = CoatCheck::new();
    /// ```
    #[inline]
    pub fn new() -> CoatCheck<V> {
        CoatCheck::with_capacity(0)
    }

    /// Constructs a new, empty `CoatCheck<T>` with the specified capacity.
    ///
    /// The coat check will be able to hold exactly `capacity` elements without reallocating. If
    /// `capacity` is 0, the coat check will not allocate.
    ///
    /// It is important to note that this function does not specify the *length* of the returned
    /// coat check, but only the *capacity*. (For an explanation of the difference between length and
    /// capacity, see the main `Vec<T>` docs in the `std::vec` module, 'Capacity and reallocation'.)
    ///
    /// # Examples
    ///
    /// ```
    /// use coatcheck::CoatCheck;
    ///
    /// let mut cc: CoatCheck<i32> = CoatCheck::with_capacity(10);
    ///
    /// // The coat check contains no items, even though it has capacity for more
    /// assert_eq!(cc.len(), 0);
    ///
    /// // These are all done without reallocating...
    /// for i in 0i32..10 {
    ///     let _ = cc.check(i);
    /// }
    ///
    /// // ...but this may make the coat check reallocate
    /// cc.check(11);
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> CoatCheck<V> {
        CoatCheck {
            tag: random(),
            data: Vec::with_capacity(capacity),
            next_free: 0,
            size: 0,
        }
    }

    /// Returns the number of elements the coat check can hold without reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use coatcheck::CoatCheck;
    ///
    /// let cc: CoatCheck<i32> = CoatCheck::with_capacity(10);
    /// assert_eq!(cc.capacity(), 10);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// The number of checked items.
    ///
    /// # Examples
    ///
    /// ```
    /// use coatcheck::CoatCheck;
    ///
    /// let mut cc = CoatCheck::new();
    /// assert_eq!(cc.len(), 0);
    /// cc.check("a");
    /// assert_eq!(cc.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Reserves capacity for at least `additional` more elements to be checked into the given
    /// `CoatCheck<T>`. The collection may reserve more space to avoid frequent reallocations.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity overflows `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use coatcheck::CoatCheck;
    ///
    /// let mut cc: CoatCheck<i32> = CoatCheck::new();
    /// let t1 = cc.check(1);
    /// cc.reserve(10);
    /// assert!(cc.capacity() >= 11);
    /// ```
    pub fn reserve(&mut self, additional: usize) {
        let extra_space = self.data.len() - self.len();
        if extra_space < additional {
            self.data.reserve(additional - extra_space)
        }
    }

    /// Reserves the minimum capacity for exactly `additional` more elements to be check into the
    /// given `CoatCheck<T>`. Does nothing if the capacity is already sufficient.
    ///
    /// Note that the allocator may give the collection more space than it requests. Therefore
    /// capacity can not be relied upon to be precisely minimal. Prefer `reserve` if future
    /// check-ins are expected.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity overflows `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use coatcheck::CoatCheck;
    ///
    /// let mut cc: CoatCheck<i32> = CoatCheck::new();
    /// let t1 = cc.check(1);
    /// cc.reserve_exact(10);
    /// assert!(cc.capacity() >= 11);
    /// ```
    pub fn reserve_exact(&mut self, additional: usize){
        let extra_space = self.data.len() - self.len();
        if extra_space < additional {
            self.data.reserve_exact(additional - extra_space)
        }
    }

    /// Check a value in and get a `Ticket `in exchange.
    ///
    /// This ticket cannot be copied, cloned, or forged but can be used to reference (`get*`) or
    /// claim values from this CoatCheck.
    ///
    /// *Panics* if the size of the `CoatCheck<V>` would overflow `usize::MAX`.
    pub fn check(&mut self, value: V) -> Ticket {
        let loc = self.next_free;
        debug_assert!(loc <= self.data.len());

        if self.next_free == self.data.len() {
            self.data.push(Full(value));
            self.next_free = self.next_free.checked_add(1).unwrap();
        } else {
            self.next_free = self.data[loc].fill(value);
        }
        self.size += 1;
        Ticket { tag: self.tag, index: loc }
    }

    /// Check all the items in an iterator and get tickets back.
    ///
    /// *Warning:* If you don't take your tickets (collect them from the iterator), you're items
    /// won't be checked.
    #[inline]
    pub fn check_all<I>(&mut self, iter: I) -> Tickets<I, V> where I: Iterator<Item=V> {
        let (lower, _) = iter.size_hint();
        self.reserve(lower);
        Tickets { iter: iter, cc: self }
    }

    /// Iterate over the items in this `CoatCheck<V>`.
    #[inline]
    pub fn iter<'a>(&'a self) -> Iter<'a, V> {
        Iter {
            remaining: self.len(),
            inner: self.data.iter().filter_map(Entry::<V>::full_ref as fn(&'a Entry<V>) -> Option<&'a V>),
        }
    }

    /// Mutably iterate over the items in this `CoatCheck<V>`.
    #[inline]
    pub fn iter_mut<'a>(&'a mut self) -> IterMut<'a, V> {
        IterMut {
            remaining: self.len(),
            inner: self.data.iter_mut().filter_map(Entry::<V>::full_mut as fn(&'a mut Entry<V>) -> Option<&'a mut V>)
        }
    }

    /// Creates a consuming iterator, that is, one that moves each value out of the coat check (from
    /// start to end). The coat check cannot be used after calling this.
    #[inline]
    pub fn into_iter(self) -> IntoIter<V> {
        IntoIter {
            remaining: self.len(),
            inner: self.data.into_iter().filter_map(Entry::<V>::full as fn(Entry<V>) -> Option<V>)
        }
    }

    /// Check if a ticket belongs to this `CoatCheck<V>`.
    ///
    /// Returns true if the ticket belongs to this `CoatCheck<V>`.
    #[inline]
    pub fn contains_ticket(&self, ticket: &Ticket) -> bool {
        // Tickets can't be forged or duplicated so a matching tag SHOULD mean that the ticket is
        // valid.
        debug_assert!(self.data[ticket.index].is_full());
        ticket.tag == self.tag
    }

    /// Check if this `CoatCheck<V>` is empty.
    ///
    /// Returns `true` if this `CoatCheck<V>` is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Claim an item.
    ///
    /// Returns `Ok(value)` if the ticket belongs to this `CoatCheck<V>` (eating the ticket).
    /// Returns `Err(ticket)` if the ticket belongs to another `CoatCheck<V>` (returning the ticket).
    pub fn claim(&mut self, ticket: Ticket) -> Result<V, ClaimError> {
        if ticket.tag != self.tag {
            Err(ClaimError { ticket: ticket, kind: ErrorKind::WrongCoatCheck })
        } else {
            let value = self.data[ticket.index].empty(self.next_free);
            self.next_free = ticket.index;
            self.size -= 1;
            Ok(value)
        }
    }

    /// Get a reference to the value matching this ticket.
    ///
    /// Returns `Ok(&value)` if the ticket belongs to this `CoatCheck<V>`.
    /// Returns `Err(())` if the ticket belongs to another `CoatCheck<V>`.
    pub fn get(&self, ticket: &Ticket) -> Result<&V, AccessError> {
        if ticket.tag != self.tag {
            Err(AccessError { kind: ErrorKind::WrongCoatCheck })
        } else {
            match self.data.index(&ticket.index) {
                &Full(ref v) => Ok(v),
                _ => panic!("forged ticket"),
            }
        }
    }

    /// Get a mutable reference to the value matching this ticket.
    ///
    /// Returns `Ok(&mut value)` if the ticket belongs to this `CoatCheck<V>`.
    /// Returns `Err(())` if the ticket belongs to another `CoatCheck<V>`.
    pub fn get_mut(&mut self, ticket: &Ticket) -> Result<&mut V, AccessError> {
        if ticket.tag != self.tag {
            Err(AccessError { kind: ErrorKind::WrongCoatCheck })
        } else {
            match self.data.index_mut(&ticket.index) {
                &mut Full(ref mut v) => Ok(v),
                _ => panic!("forged ticket"),
            }
        }
    }
}

impl<V> fmt::Debug for CoatCheck<V> where V: fmt::Debug {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "{{"));
        for (i, v) in self.iter().enumerate() {
            if i != 0 { try!(write!(f, ", ")); }
            try!(write!(f, "{:?}", *v));
        }
        write!(f, "}}")
    }
}

impl<V> Index<Ticket> for CoatCheck<V> {
    type Output = V;
    #[inline]
    fn index(&self, ticket: &Ticket) -> &V {
        self.get(ticket).ok().expect("ticket for wrong CoatCheck")
    }
}

impl<V> IndexMut<Ticket> for CoatCheck<V> {
    type Output = V;
    #[inline]
    fn index_mut(&mut self, ticket: &Ticket) -> &mut V {
        self.get_mut(ticket).ok().expect("ticket for wrong CoatCheck")
    }
}

impl<V> Default for CoatCheck<V> {
    #[inline]
    fn default() -> CoatCheck<V> {
        CoatCheck::new()
    }
}

#[cfg(test)]
mod test {
    extern crate test;
    use super::*;
    use std::collections::HashMap;
    use self::test::Bencher;

    #[test]
    fn two_cc() {
        let mut c1 = CoatCheck::new();
        let mut c2 = CoatCheck::new();

        let t1 = c1.check(1);
        let t2 = c1.check(2);
        assert_eq!(c1[t1], 1);
        assert_eq!(c1[t2], 2);
        assert_eq!(c1[t1], 1);
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
            assert_eq!(iter.next().cloned(), Some(0));
            assert_eq!(iter.next().cloned(), Some(1));
            assert_eq!(iter.next(), None);
        }

        {
            let mut iter = cc.iter_mut();
            assert_eq!(iter.next().cloned(), Some(0));
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
        let tickets: Vec<Ticket> = cc.check_all(0us..10).collect();
        for (i, t) in tickets.iter().enumerate() {
            assert_eq!(cc[*t], i);
        }
        cc[tickets[2]] = 1;
        assert_eq!(cc[tickets[2]], 1);
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

    #[bench]
    fn bench_hash_map(b: &mut Bencher) {
        b.iter(|| {
            let mut map = HashMap::new();
            let mut res = Vec::with_capacity(10);
            for i in 0..10 {
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
            let mut cc = CoatCheck::new();
            let mut res = Vec::with_capacity(10);
            for _ in 0..10 {
                res.push(cc.check("something"));
            }
            for t in res.into_iter() {
                let _ = cc.claim(t);
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
}
