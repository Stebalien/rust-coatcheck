//! # CoatCheck
//!
//! CoatCheck is a library for storing values and referencing them by "handles"
//! (`Tickets`). This library is primarily designed for when one needs to be able
//! to "register" an object with a system and refer to it after registration but
//! one doesn't need to actually access the object.
//!
//! ## Explanation by example
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
//! However, this is kind of an abuse of a hash table.
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
//! Pros:
//!  * Ownership: Preventing duplication of the ticket preserves ownership of the
//!    value to an extent. The value can still be stolen by destroying the
//!    CoatCheck but that's the only way to get it out without the ticket.
//!  * Safety: As long as you use the ticket in the right coat check, the index
//!    operator will never panic.
//! Cons:
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

impl fmt::Show for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Ticket")
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

pub struct IntoIter<V> {
    inner: iter::FilterMap<Entry<V>, V, vec::IntoIter<Entry<V>>, fn(Entry<V>) -> Option<V>>,
    remaining: usize
}

impl<V> ExactSizeIterator for IntoIter<V> {
    #[inline]
    fn len(&self) -> usize {
        self.remaining
    }
}

impl<V> Iterator for IntoIter<V> {
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

impl<V> DoubleEndedIterator for IntoIter<V> {
    fn next_back(&mut self) -> Option<V> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.inner.next_back()
        } else {
            None
        }
    }
}

pub struct Iter<'a, V> where V: 'a {
    inner: iter::FilterMap<&'a Entry<V>, &'a V, slice::Iter<'a, Entry<V>>, fn(&'a Entry<V>) -> Option<&'a V>>,
    remaining: usize
}

impl<'a, V> ExactSizeIterator for Iter<'a, V> where V: 'a {
    #[inline]
    fn len(&self) -> usize {
        self.remaining
    }
}

impl<'a, V> Iterator for Iter<'a, V> where V: 'a {
    type Item = &'a V;
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

impl<'a, V> DoubleEndedIterator for Iter<'a, V> where V: 'a {
    fn next_back(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.inner.next_back()
        } else {
            None
        }
    }
}

pub struct IterMut<'a, V> where V: 'a {
    inner: iter::FilterMap<&'a mut Entry<V>, &'a mut V, slice::IterMut<'a, Entry<V>>, fn(&'a mut Entry<V>) -> Option<&'a mut V>>,
    remaining: usize
}

impl<'a, V> ExactSizeIterator for IterMut<'a, V> where V: 'a {
    #[inline]
    fn len(&self) -> usize {
        self.remaining
    }
}

impl<'a, V> Iterator for IterMut<'a, V> where V: 'a {
    type Item = &'a mut V;
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

impl<'a, V> DoubleEndedIterator for IterMut<'a, V> where V: 'a {
    fn next_back(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.inner.next_back()
        } else {
            None
        }
    }
}

pub struct CoatCheck<V> {
    tag: usize,
    data: Vec<Entry<V>>,
    size: usize,
    next_free: usize,
}

/// A data structure storing values indexed by tickets.
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
    pub fn iter(&self) -> Iter<V> {
        Iter { remaining: self.len(), inner: self.data.iter().filter_map(Entry::<V>::full_ref) }
    }

    /// Mutably iterate over the items in this `CoatCheck<V>`.
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<V> {
        IterMut { remaining: self.len(), inner: self.data.iter_mut().filter_map(Entry::<V>::full_mut) }
    }

    /// Creates a consuming iterator, that is, one that moves each value out of the coat check (from
    /// start to end). The coat check cannot be used after calling this.
    #[inline]
    pub fn into_iter(self) -> IntoIter<V> {
        IntoIter { remaining: self.len(), inner: self.data.into_iter().filter_map(Entry::<V>::full) }
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
    pub fn claim(&mut self, ticket: Ticket) -> Result<V, Ticket> {
        if ticket.tag != self.tag {
            Err(ticket)
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
    pub fn get(&self, ticket: &Ticket) -> Result<&V, ()> {
        if ticket.tag != self.tag {
            Err(())
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
    pub fn get_mut(&mut self, ticket: &Ticket) -> Result<&mut V, ()> {
        if ticket.tag != self.tag {
            Err(())
        } else {
            match self.data.index_mut(&ticket.index) {
                &mut Full(ref mut v) => Ok(v),
                _ => panic!("forged ticket"),
            }
        }
    }
}

impl<V> fmt::Show for CoatCheck<V> where V: fmt::Show {
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

#[test]
fn test() {
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
    println!("{:?}", c2);

    {
        let mut iter = c2.iter();
        assert_eq!(iter.next().cloned(), Some(4));
        assert_eq!(iter.next().cloned(), Some(5));
        assert_eq!(iter.next(), None);
    }

    {
        let mut iter = c2.iter_mut();
        assert_eq!(iter.next().cloned(), Some(4));
        let it = iter.next().unwrap();
        assert_eq!(it, &mut 5);
        *it = 6;
        assert_eq!(iter.next(), None);
    }

    {
        let mut iter = c2.into_iter();
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), None)
    }
}

#[test]
fn test_check_all() {
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

