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
//! ```
//! use coatcheck::{CoatCheck, Ticket};
//! use std::convert::From;
//!
//! let mut cc = CoatCheck::new();
//!
//! // Check two values.
//! let ticket1 = cc.check("my value");
//! let ticket2 = cc.check("my other value");
//!
//! // Look at the first one.
//! println!("{}", cc[&ticket1]);
//!
//! // Claim the second one.
//! println!("{}", cc.claim(ticket2).unwrap());
//!
//! // Claiming again will fail at compile time.
//! // println!("{}", cc.claim(ticket2).unwrap());
//!
//! // Drain the items into a vector.
//! let items: Vec<&str> = cc.into_iter().collect();
//! assert_eq!(items[0], "my value");
//!
//! // Create a second coat check:
//! let mut cc2: CoatCheck<&str> = CoatCheck::new();
//!
//! // `ticket1` was never claimed so let's try claiming it in this coat check...
//! let ticket: Ticket = From::from(cc2.claim(ticket1).unwrap_err());
//! // It fails and returns the ticket.
//! ```
//!
//! ## Use Case
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
extern crate snowflake;

use std::fmt;
use std::vec;
use std::ops::{Index, IndexMut};
use std::slice;
use std::iter;
use std::mem;
use std::convert::From;
use std::error::Error as ErrorTrait;

use snowflake::ProcessUniqueId;

use Entry::*;

enum Entry<V> {
    Empty(usize /* next free index */),
    Full(V),
}

impl<V> Entry<V> {

    /// Take the value if it exists.
    #[inline]
    fn full(self) -> Option<V> {
        match self {
            Full(value) => Some(value),
            Empty(_) => None
        }
    }

    /// Get an optional reference to the value.
    #[inline]
    fn full_ref(&self) -> Option<&V> {
        match self {
            &Full(ref value) => Some(value),
            _ => None
        }
    }

    /// Get an optional mutable reference to the value.
    #[inline]
    fn full_mut(&mut self) -> Option<&mut V> {
        match self {
            &mut Full(ref mut value) => Some(value),
            _ => None
        }
    }

    /// Is the entry full
    #[inline]
    fn is_full(&self) -> bool {
        match self {
            &Full(_) => true,
            _ => false,
        }
    }

    /// Fill an empty entry with a value and return the next free index.
    #[inline]
    fn fill(&mut self, value: V) -> usize {
        match mem::replace(self, Full(value)) {
            Empty(next_free) => next_free,
            _ => panic!("expected no entry"),
        }
    }

    /// Empty a full entry with one setting the next free index and returning the value.
    #[inline]
    fn empty(&mut self, next_free: usize) -> V {
        match mem::replace(self, Empty(next_free)) {
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
    tag: ProcessUniqueId,
    index: usize,
}

impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Ticket")
    }
}

/// Coat check error types
#[derive(Clone, Copy)]
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
    /// The error kind.
    pub kind: ErrorKind,
    /// The ticket used in the failed claim.
    pub ticket: Ticket,
}

impl ErrorTrait for ClaimError {
    fn description(&self) -> &str {
        self.kind.description()
    }
}

impl fmt::Display for ClaimError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ClaimError: {}", self.description())
    }
}

impl fmt::Debug for ClaimError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl From<ClaimError> for Ticket {
    fn from(e: ClaimError) -> Ticket {
        e.ticket
    }
}

/// The error yielded an access fails.
#[derive(Clone, Copy)]
pub struct AccessError {
    /// The error kind.
    pub kind: ErrorKind,
}

impl ErrorTrait for AccessError {
    fn description(&self) -> &str {
        self.kind.description()
    }
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AccessError: {}", self.description())
    }
}

impl fmt::Debug for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Iterator that checks-in values in exchange for tickets.
pub struct Tickets<'a, I> where I: Iterator, <I as Iterator>::Item: 'a {
    iter: I,
    cc: &'a mut CoatCheck<<I as Iterator>::Item>,
}

impl<'a, I> Iterator for Tickets<'a, I> where I: Iterator, <I as Iterator>::Item: 'a {
    type Item = Ticket;

    fn next(&mut self) -> Option<Ticket> {
        self.iter.next().map(|v| self.cc.check(v))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a, I> ExactSizeIterator for Tickets<'a, I> where
    I: ExactSizeIterator,
    <I as Iterator>::Item: 'a
{ }

impl<'a, I> DoubleEndedIterator for Tickets<'a, I> where
    I: DoubleEndedIterator,
    <I as Iterator>::Item: 'a
{
    fn next_back(&mut self) -> Option<Ticket> {
        self.iter.next_back().map(|v| self.cc.check(v))
    }
}

#[doc(hidden)]
struct GenericIter<I> where I: Iterator {
    inner: I,
    remaining: usize,

}

impl<I> ExactSizeIterator for GenericIter<I> where I: Iterator {
    #[inline]
    fn len(&self) -> usize {
        self.remaining
    }
}

impl<I> Iterator for GenericIter<I> where I: Iterator {
    type Item = <I as Iterator>::Item;
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
pub type IntoIter<V> = GenericIter<iter::FilterMap<
    vec::IntoIter<Entry<V>>, fn(Entry<V>) -> Option<V>
>>;

#[doc(hidden)]
pub type Iter<'a, V> = GenericIter< iter::FilterMap<
    slice::Iter<'a, Entry<V>>, fn(&'a Entry<V>) -> Option<&'a V>
>>;

#[doc(hidden)]
pub type IterMut<'a, V> = GenericIter<iter::FilterMap<
    slice::IterMut<'a, Entry<V>>,
    fn(&'a mut Entry<V>) -> Option<&'a mut V>
>>;

/// A data structure storing values indexed by tickets.
pub struct CoatCheck<V> {
    tag: ProcessUniqueId,
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
    pub fn new() -> Self {
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
    pub fn with_capacity(capacity: usize) -> Self {
        CoatCheck {
            tag: ProcessUniqueId::new(),
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

        self.next_free = if self.next_free == self.data.len() {
            self.data.push(Full(value));
            self.next_free.checked_add(1).unwrap()
        } else {
            // Safe because we've recorded that it is safe.
            unsafe { self.data.get_unchecked_mut(loc) }.fill(value)
        };
        self.size += 1;
        Ticket { tag: self.tag, index: loc }
    }

    /// Check all the items in an iterator and get tickets back.
    ///
    /// *Warning:* If you don't take your tickets (collect them from the iterator), you're items
    /// won't be checked.
    #[inline]
    pub fn check_all<I>(&mut self, iter: I) -> Tickets<I> where I: Iterator<Item=V> {
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
    /// Returns `Err(ClaimError)` if the ticket belongs to another `CoatCheck<V>` (returning the
    /// ticket inside of the ClaimError).
    pub fn claim(&mut self, ticket: Ticket) -> Result<V, ClaimError> {
        match ticket {
            Ticket { tag, index } if tag == self.tag => {
                // Safe because, if we've handed out the ticket, this slot must exist.
                let value = unsafe { self.data.get_unchecked_mut(index) }.empty(self.next_free);
                self.next_free = index;
                self.size -= 1;
                Ok(value)
            },
            _ => Err(ClaimError { ticket: ticket, kind: ErrorKind::WrongCoatCheck })
        }
    }

    /// Get a reference to the value matching this ticket.
    ///
    /// Returns `Ok(&value)` if the ticket belongs to this `CoatCheck<V>`.
    /// Returns `Err(AccessError)` if the ticket belongs to another `CoatCheck<V>`.
    pub fn get(&self, ticket: &Ticket) -> Result<&V, AccessError> {
        match ticket {
            &Ticket { tag, index } if tag == self.tag => match unsafe {
                // Safe because, if we've handed out the ticket, this slot must exist.
                self.data.get_unchecked(index)
            } {
                &Full(ref v) => Ok(v),
                _ => panic!("forged ticket"),
            },
            _ =>  Err(AccessError { kind: ErrorKind::WrongCoatCheck })
        }
    }

    /// Get a mutable reference to the value matching this ticket.
    ///
    /// Returns `Ok(&mut value)` if the ticket belongs to this `CoatCheck<V>`.
    /// Returns `Err(AccessError)` if the ticket belongs to another `CoatCheck<V>`.
    pub fn get_mut(&mut self, ticket: &Ticket) -> Result<&mut V, AccessError> {
        match ticket {
            &Ticket { tag, index } if tag == self.tag => match unsafe {
                // Safe because, if we've handed out the ticket, this slot must exist.
                self.data.get_unchecked_mut(index)
            } {
                &mut Full(ref mut v) => Ok(v),
                _ => panic!("forged ticket"),
            },
            _ =>  Err(AccessError { kind: ErrorKind::WrongCoatCheck })
        }
    }
}

impl<V> IntoIterator for CoatCheck<V> {
    type Item = V;
    type IntoIter = IntoIter<V>;

    /// Creates a consuming iterator, that is, one that moves each value out of the coat check (from
    /// start to end). The coat check cannot be used after calling this.
    #[inline]
    fn into_iter(self) -> IntoIter<V> {
        IntoIter {
            remaining: self.len(),
            inner: self.data.into_iter().filter_map(Entry::<V>::full as fn(Entry<V>) -> Option<V>)
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

impl<'a, V> Index<&'a Ticket> for CoatCheck<V> {
    type Output = V;
    #[inline]
    fn index(&self, ticket: &Ticket) -> &V {
        self.get(ticket).ok().expect("ticket for wrong CoatCheck")
    }
}

impl<'a, V> IndexMut<&'a Ticket> for CoatCheck<V> {
    #[inline]
    fn index_mut(&mut self, ticket: &Ticket) -> &mut V {
        self.get_mut(ticket).ok().expect("ticket for wrong CoatCheck")
    }
}

impl<V> Default for CoatCheck<V> {
    #[inline]
    fn default() -> Self {
        CoatCheck::new()
    }
}
