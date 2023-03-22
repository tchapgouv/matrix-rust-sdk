//! The logic to generate Sliding Sync list requests.
//!
//! Depending on the [`SlidingSyncMode`], the generated requests aren't the
//! same.
//!
//! In [`SlidingSyncMode::Selective`], it's pretty straightforward:
//!
//! * There is a set of ranges,
//! * Each request asks to load the particular ranges.
//!
//! In [`SlidingSyncMode::PagingFullSync`]:
//!
//! * There is a `batch_size`,
//! * Each request asks to load a new successive range containing exactly
//!   `batch_size` rooms.
//!
//! In [`SlidingSyncMode::GrowingFullSync]:
//!
//! * There is a `batch_size`,
//! * Each request asks to load a new range, always starting from 0, but where
//!   the end is incremented by `batch_size` everytime.
//!
//! The number of rooms to load is capped by the
//! [`SlidingSyncList::maximum_number_of_rooms`], i.e. the real number of
//! rooms it is possible to load. This value comes from the server.
//!
//! The number of rooms to load can _also_ be capped by the
//! [`SlidingSyncList::full_sync_maximum_number_of_rooms_to_fetch`], i.e. a
//! user-specified limit representing the maximum number of rooms the user
//! actually wants to load.

use std::cmp::min;

use ruma::UInt;

/// The kind of request generator.
#[derive(Debug)]
pub(super) enum SlidingSyncListRequestGeneratorKind {
    /// Growing-mode (see [`SlidingSyncMode`]).
    GrowingFullSync {
        /// Size of the batch, used to grow the range to fetch more rooms.
        batch_size: u32,
        /// Maximum number of rooms to fetch (see
        /// [`SlidingSyncList::full_sync_maximum_number_of_rooms_to_fetch`]).
        maximum_number_of_rooms_to_fetch: Option<u32>,
        /// Number of rooms that have been already fetched.
        number_of_fetched_rooms: u32,
        /// Whether all rooms have been loaded.
        fully_loaded: bool,
    },

    /// Paging-mode (see [`SlidingSyncMode`]).
    PagingFullSync {
        /// Size of the batch, used to grow the range to fetch more rooms.
        batch_size: u32,
        /// Maximum number of rooms to fetch (see
        /// [`SlidingSyncList::full_sync_maximum_number_of_rooms_to_fetch`]).
        maximum_number_of_rooms_to_fetch: Option<u32>,
        /// Number of rooms that have been already fetched.
        number_of_fetched_rooms: u32,
        /// Whether all romms have been loaded.
        fully_loaded: bool,
    },

    /// Selective-mode (see [`SlidingSyncMode`]).
    Selective,
}

/// A request generator for [`SlidingSyncList`].
#[derive(Debug)]
pub(in super::super) struct SlidingSyncListRequestGenerator {
    /// The current range used by this request generator.
    pub(super) ranges: Vec<(UInt, UInt)>,
    /// The kind of request generator.
    pub(super) kind: SlidingSyncListRequestGeneratorKind,
}

impl SlidingSyncListRequestGenerator {
    /// Create a new request generator configured for paging-mode.
    pub(super) fn new_paging_full_sync(
        batch_size: u32,
        maximum_number_of_rooms_to_fetch: Option<u32>,
    ) -> Self {
        Self {
            ranges: Vec::new(),
            kind: SlidingSyncListRequestGeneratorKind::PagingFullSync {
                batch_size,
                maximum_number_of_rooms_to_fetch,
                number_of_fetched_rooms: 0,
                fully_loaded: false,
            },
        }
    }

    /// Create a new request generator configured for growing-mode.
    pub(super) fn new_growing_full_sync(
        batch_size: u32,
        maximum_number_of_rooms_to_fetch: Option<u32>,
    ) -> Self {
        Self {
            ranges: Vec::new(),
            kind: SlidingSyncListRequestGeneratorKind::GrowingFullSync {
                batch_size,
                maximum_number_of_rooms_to_fetch,
                number_of_fetched_rooms: 0,
                fully_loaded: false,
            },
        }
    }

    /// Create a new request generator configured for selective-mode.
    pub(super) fn new_selective() -> Self {
        Self { ranges: Vec::new(), kind: SlidingSyncListRequestGeneratorKind::Selective }
    }

    #[cfg(test)]
    fn is_fully_loaded(&self) -> bool {
        match self.kind {
            SlidingSyncListRequestGeneratorKind::PagingFullSync { fully_loaded, .. }
            | SlidingSyncListRequestGeneratorKind::GrowingFullSync { fully_loaded, .. } => {
                fully_loaded
            }
            SlidingSyncListRequestGeneratorKind::Selective => true,
        }
    }
}

pub(super) fn create_range(
    start: u32,
    desired_size: u32,
    maximum_number_of_rooms_to_fetch: Option<u32>,
    maximum_number_of_rooms: Option<u32>,
) -> Option<(UInt, UInt)> {
    // Calculate the range.
    // The `start` bound is given. Let's calculate the `end` bound.

    // The `end`, by default, is `start` + `desired_size`.
    let mut end = start + desired_size;

    // But maybe the user has defined a maximum number of rooms to fetch? In this
    // case, take the minimum of the two.
    if let Some(maximum_number_of_rooms_to_fetch) = maximum_number_of_rooms_to_fetch {
        end = min(end, maximum_number_of_rooms_to_fetch);
    }

    // But there is more! The server can tell us what is the maximum number of rooms
    // fulfilling a particular list. For example, if the server says there is 42
    // rooms for a particular list, with a `start` of 40 and a `batch_size` of 20,
    // the range must be capped to `[40; 46]`; the range `[40; 60]` would be invalid
    // and could be rejected by the server.
    if let Some(maximum_number_of_rooms) = maximum_number_of_rooms {
        end = min(end, maximum_number_of_rooms);
    }

    // Finally, because the bounds of the range are inclusive, 1 is subtracted.
    end = end.saturating_sub(1);

    // Make sure `start` is smaller than `end`. It can happen if `start` is greater
    // than `maximum_number_of_rooms_to_fetch` or `maximum_number_of_rooms`.
    if start > end {
        return None;
    }

    Some((start.into(), end.into()))
}

#[cfg(test)]
mod tests {
    use ruma::uint;

    use super::{
        super::{SlidingSyncList, SlidingSyncState},
        *,
    };

    #[test]
    fn test_create_range_from() {
        // From 0, we want 100 items.
        assert_eq!(create_range(0, 100, None, None), Some((uint!(0), uint!(99))));

        // From 100, we want 100 items.
        assert_eq!(create_range(100, 100, None, None), Some((uint!(100), uint!(199))));

        // From 0, we want 100 items, but there is a maximum number of rooms to fetch
        // defined at 50.
        assert_eq!(create_range(0, 100, Some(50), None), Some((uint!(0), uint!(49))));

        // From 49, we want 100 items, but there is a maximum number of rooms to fetch
        // defined at 50. There is 1 item to load.
        assert_eq!(create_range(49, 100, Some(50), None), Some((uint!(49), uint!(49))));

        // From 50, we want 100 items, but there is a maximum number of rooms to fetch
        // defined at 50.
        assert_eq!(create_range(50, 100, Some(50), None), None);

        // From 0, we want 100 items, but there is a maximum number of rooms defined at
        // 50.
        assert_eq!(create_range(0, 100, None, Some(50)), Some((uint!(0), uint!(49))));

        // From 49, we want 100 items, but there is a maximum number of rooms defined at
        // 50. There is 1 item to load.
        assert_eq!(create_range(49, 100, None, Some(50)), Some((uint!(49), uint!(49))));

        // From 50, we want 100 items, but there is a maximum number of rooms defined at
        // 50.
        assert_eq!(create_range(50, 100, None, Some(50)), None);

        // From 0, we want 100 items, but there is a maximum number of rooms to fetch
        // defined at 75, and a maximum number of rooms defined at 50.
        assert_eq!(create_range(0, 100, Some(75), Some(50)), Some((uint!(0), uint!(49))));

        // From 0, we want 100 items, but there is a maximum number of rooms to fetch
        // defined at 50, and a maximum number of rooms defined at 75.
        assert_eq!(create_range(0, 100, Some(50), Some(75)), Some((uint!(0), uint!(49))));
    }

    macro_rules! assert_request_and_response {
        (
            list = $list:ident,
            maximum_number_of_rooms = $maximum_number_of_rooms:expr,
            $(
                next => {
                    ranges = $( [ $range_start:literal ; $range_end:literal ] ),+ ,
                    is_fully_loaded = $is_fully_loaded:expr,
                    list_state = $list_state:ident,
                }
            ),*
            $(,)*
        ) => {
            // That's the initial state.
            assert_eq!($list.state(), SlidingSyncState::NotLoaded);

            $(
                {
                    // Generate a new request.
                    let request = $list.next_request().unwrap();

                    assert_eq!(request.ranges, [ $( (uint!( $range_start ), uint!( $range_end )) ),* ]);

                    // Fake a response.
                    let _ = $list.handle_response($maximum_number_of_rooms, &vec![], &vec![]);

                    assert_eq!($list.inner.request_generator.read().unwrap().is_fully_loaded(), $is_fully_loaded);
                    assert_eq!($list.state(), SlidingSyncState::$list_state);
                }
            )*
        };
    }

    #[test]
    fn test_generator_paging_full_sync() {
        let mut list = SlidingSyncList::builder()
            .sync_mode(crate::SlidingSyncMode::PagingFullSync)
            .name("testing")
            .full_sync_batch_size(10)
            .build()
            .unwrap();

        assert_request_and_response! {
            list = list,
            maximum_number_of_rooms = 25,
            next => {
                ranges = [0; 9],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            next => {
                ranges = [10; 19],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            // The maximum number of rooms is reached!
            next => {
                ranges = [20; 24],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            // Now it's fully loaded, so the same request must be produced everytime.
            next => {
                ranges = [0; 24], // the range starts at 0 now!
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            next => {
                ranges = [0; 24],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
        };
    }

    #[test]
    fn test_generator_paging_full_sync_with_a_maximum_number_of_rooms_to_fetch() {
        let mut list = SlidingSyncList::builder()
            .sync_mode(crate::SlidingSyncMode::PagingFullSync)
            .name("testing")
            .full_sync_batch_size(10)
            .full_sync_maximum_number_of_rooms_to_fetch(22)
            .build()
            .unwrap();

        assert_request_and_response! {
            list = list,
            maximum_number_of_rooms = 25,
            next => {
                ranges = [0; 9],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            next => {
                ranges = [10; 19],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            // The maximum number of rooms to fetch is reached!
            next => {
                ranges = [20; 21],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            // Now it's fully loaded, so the same request must be produced everytime.
            next => {
                ranges = [0; 21], // the range starts at 0 now!
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            next => {
                ranges = [0; 21],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
        };
    }

    #[test]
    fn test_generator_growing_full_sync() {
        let mut list = SlidingSyncList::builder()
            .sync_mode(crate::SlidingSyncMode::GrowingFullSync)
            .name("testing")
            .full_sync_batch_size(10)
            .build()
            .unwrap();

        assert_request_and_response! {
            list = list,
            maximum_number_of_rooms = 25,
            next => {
                ranges = [0; 9],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            next => {
                ranges = [0; 19],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            // The maximum number of rooms is reached!
            next => {
                ranges = [0; 24],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            // Now it's fully loaded, so the same request must be produced everytime.
            next => {
                ranges = [0; 24],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            next => {
                ranges = [0; 24],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
        };
    }

    #[test]
    fn test_generator_growing_full_sync_with_a_maximum_number_of_rooms_to_fetch() {
        let mut list = SlidingSyncList::builder()
            .sync_mode(crate::SlidingSyncMode::GrowingFullSync)
            .name("testing")
            .full_sync_batch_size(10)
            .full_sync_maximum_number_of_rooms_to_fetch(22)
            .build()
            .unwrap();

        assert_request_and_response! {
            list = list,
            maximum_number_of_rooms = 25,
            next => {
                ranges = [0; 9],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            next => {
                ranges = [0; 19],
                is_fully_loaded = false,
                list_state = PartiallyLoaded,
            },
            // The maximum number of rooms is reached!
            next => {
                ranges = [0; 21],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            // Now it's fully loaded, so the same request must be produced everytime.
            next => {
                ranges = [0; 21],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            next => {
                ranges = [0; 21],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
        };
    }

    #[test]
    fn test_generator_selective() {
        let mut list = SlidingSyncList::builder()
            .sync_mode(crate::SlidingSyncMode::Selective)
            .name("testing")
            .ranges(vec![(0u32, 10), (42, 153)])
            .build()
            .unwrap();

        assert_request_and_response! {
            list = list,
            maximum_number_of_rooms = 25,
            // The maximum number of rooms is reached directly!
            next => {
                ranges = [0; 10], [42; 153],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            // Now it's fully loaded, so the same request must be produced everytime.
            next => {
                ranges = [0; 10], [42; 153],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            },
            next => {
                ranges = [0; 10], [42; 153],
                is_fully_loaded = true,
                list_state = FullyLoaded,
            }
        };
    }
}
