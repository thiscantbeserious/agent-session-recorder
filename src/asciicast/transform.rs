//! Event transformation pipeline for asciicast recordings.
//!
//! This module provides the [`Transform`] trait and [`TransformChain`] for
//! applying in-place modifications to recording events. Transforms are designed
//! for efficiency with large files (100+ MB) by mutating events in place rather
//! than creating copies.
//!
//! # Design Principles
//!
//! - **In-place mutation**: Transforms modify `Vec<Event>` directly to avoid
//!   memory copies when processing millions of events
//! - **Stateful transforms**: The `&mut self` receiver allows transforms to
//!   track state across events (e.g., cumulative time offsets)
//! - **Composable**: Multiple transforms can be chained together
//!
//! # Example
//!
//! ```
//! use agr::asciicast::{Event, Transform, TransformChain};
//!
//! /// A transform that removes all marker events.
//! struct RemoveMarkers;
//!
//! impl Transform for RemoveMarkers {
//!     fn transform(&mut self, events: &mut Vec<Event>) {
//!         events.retain(|e| !e.is_marker());
//!     }
//! }
//!
//! /// A transform that caps event delays at a maximum value.
//! struct CapDelay {
//!     max_delay: f64,
//! }
//!
//! impl Transform for CapDelay {
//!     fn transform(&mut self, events: &mut Vec<Event>) {
//!         for event in events.iter_mut() {
//!             if event.time > self.max_delay {
//!                 event.time = self.max_delay;
//!             }
//!         }
//!     }
//! }
//!
//! // Chain multiple transforms
//! let mut chain = TransformChain::new()
//!     .with(RemoveMarkers)
//!     .with(CapDelay { max_delay: 2.0 });
//!
//! let mut events = vec![
//!     Event::output(0.5, "hello"),
//!     Event::marker(0.1, "label"),
//!     Event::output(5.0, "world"),
//! ];
//!
//! chain.transform(&mut events);
//!
//! // Markers removed, delays capped
//! assert_eq!(events.len(), 2);
//! assert!(events[1].time <= 2.0);
//! ```

use super::Event;

/// A transformation that modifies events in place.
///
/// Implement this trait to create custom event transformations. The transform
/// receives mutable access to the event vector and can add, remove, or modify
/// events as needed.
///
/// # Infallibility
///
/// Transforms are infallible by design (`()` return type). Transforms that
/// encounter invalid data should either:
/// - Filter out invalid events using `Vec::retain()`
/// - Handle errors internally and continue processing
/// - Log warnings but not fail the entire transformation
pub trait Transform {
    /// Apply this transformation to the event vector.
    ///
    /// The transform has full mutable access to modify, add, or remove events.
    fn transform(&mut self, events: &mut Vec<Event>);
}

/// A chain of transforms applied in sequence.
///
/// Builds a pipeline of transformations that are applied in order. Each
/// transform sees the result of previous transforms in the chain.
///
/// # Example
///
/// ```
/// use agr::asciicast::{Event, Transform, TransformChain};
///
/// struct DoubleTime;
/// impl Transform for DoubleTime {
///     fn transform(&mut self, events: &mut Vec<Event>) {
///         for event in events.iter_mut() {
///             event.time *= 2.0;
///         }
///     }
/// }
///
/// let mut chain = TransformChain::new().with(DoubleTime);
/// let mut events = vec![Event::output(1.0, "test")];
/// chain.transform(&mut events);
/// assert!((events[0].time - 2.0).abs() < 0.001);
/// ```
pub struct TransformChain {
    transforms: Vec<Box<dyn Transform>>,
}

impl TransformChain {
    /// Create an empty transform chain.
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }

    /// Add a transform to the end of the chain.
    ///
    /// Returns self for method chaining.
    pub fn with<T: Transform + 'static>(mut self, transform: T) -> Self {
        self.transforms.push(Box::new(transform));
        self
    }

    /// Check if the chain has no transforms.
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }

    /// Get the number of transforms in the chain.
    pub fn len(&self) -> usize {
        self.transforms.len()
    }
}

impl Default for TransformChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform for TransformChain {
    /// Apply all transforms in sequence.
    fn transform(&mut self, events: &mut Vec<Event>) {
        for transform in &mut self.transforms {
            transform.transform(events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RemoveMarkers;

    impl Transform for RemoveMarkers {
        fn transform(&mut self, events: &mut Vec<Event>) {
            events.retain(|e| !e.is_marker());
        }
    }

    struct CapDelay {
        max_delay: f64,
    }

    impl Transform for CapDelay {
        fn transform(&mut self, events: &mut Vec<Event>) {
            for event in events.iter_mut() {
                if event.time > self.max_delay {
                    event.time = self.max_delay;
                }
            }
        }
    }

    struct CountingTransform {
        count: usize,
    }

    impl Transform for CountingTransform {
        fn transform(&mut self, events: &mut Vec<Event>) {
            self.count += events.len();
        }
    }

    #[test]
    fn remove_markers_transform() {
        let mut events = vec![
            Event::output(0.1, "hello"),
            Event::marker(0.1, "test"),
            Event::output(0.2, "world"),
        ];

        let mut transform = RemoveMarkers;
        transform.transform(&mut events);

        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| !e.is_marker()));
    }

    #[test]
    fn cap_delay_transform() {
        let mut events = vec![
            Event::output(0.5, "fast"),
            Event::output(10.0, "slow"),
            Event::output(1.0, "medium"),
        ];

        let mut transform = CapDelay { max_delay: 2.0 };
        transform.transform(&mut events);

        assert!((events[0].time - 0.5).abs() < 0.001);
        assert!((events[1].time - 2.0).abs() < 0.001);
        assert!((events[2].time - 1.0).abs() < 0.001);
    }

    #[test]
    fn transform_chain_applies_in_order() {
        let mut events = vec![
            Event::output(0.5, "hello"),
            Event::marker(0.1, "label"),
            Event::output(5.0, "world"),
        ];

        let mut chain = TransformChain::new()
            .with(RemoveMarkers)
            .with(CapDelay { max_delay: 2.0 });

        chain.transform(&mut events);

        // Markers removed first, then delays capped
        assert_eq!(events.len(), 2);
        assert!((events[1].time - 2.0).abs() < 0.001);
    }

    #[test]
    fn empty_chain_does_nothing() {
        let mut events = vec![Event::output(1.0, "test")];
        let original_len = events.len();

        let mut chain = TransformChain::new();
        chain.transform(&mut events);

        assert_eq!(events.len(), original_len);
    }

    #[test]
    fn chain_len_and_is_empty() {
        let chain = TransformChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        let chain = chain.with(RemoveMarkers);
        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn stateful_transform() {
        let mut events = vec![Event::output(0.1, "a"), Event::output(0.2, "b")];

        let mut transform = CountingTransform { count: 0 };
        transform.transform(&mut events);

        assert_eq!(transform.count, 2);
    }
}
