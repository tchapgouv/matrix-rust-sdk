// Copyright 2023 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for that specific language governing permissions and
// limitations under the License.

//! States and actions for the `RoomList` state machine.

use std::{
    future::ready,
    time::{Duration, Instant},
};

use matrix_sdk::{sliding_sync::Range, SlidingSync, SlidingSyncMode};

use super::Error;

pub const ALL_ROOMS_LIST_NAME: &str = "all_rooms";

/// The state of the [`super::RoomList`].
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// That's the first initial state.
    Init,

    /// At this state, the first rooms have been synced.
    SettingUp,

    /// At this state, the system is recovering from `Error` or `Terminated`.
    /// It's similar to `SettingUp` but some lists may already exist, actions
    /// are then slightly different.
    Recovering,

    /// At this state, all rooms are syncing.
    Running,

    /// At this state, the sync has been stopped because an error happened.
    Error { from: Box<State> },

    /// At this state, the sync has been stopped because it was requested.
    Terminated { from: Box<State> },
}

const DEFAULT_DELAY_BEFORE_RECOVER: Duration = Duration::from_secs(1800);

/// The state machine used to transition between the [`State`]s.
#[derive(Clone, Debug, PartialEq)]
pub struct StateMachine {
    last_sync_date: Instant,
    delay_before_recover: Duration,
}

impl StateMachine {
    pub(super) fn new() -> Self {
        StateMachine {
            last_sync_date: Instant::now(),
            delay_before_recover: DEFAULT_DELAY_BEFORE_RECOVER,
        }
    }

    /// Transition to the next state, and execute the associated transition's
    /// [`Actions`].
    pub(super) async fn next(
        &self,
        current: State,
        sliding_sync: &SlidingSync,
    ) -> Result<State, Error> {
        use State::*;

        let next_state = match current {
            Init => SettingUp,

            SettingUp | Recovering => {
                set_all_rooms_to_growing_sync_mode(sliding_sync).await?;
                Running
            }

            Running => {
                // We haven't sync for a while so we should go back to recovering
                if self.last_sync_date.elapsed() > self.delay_before_recover {
                    set_all_rooms_to_selective_sync_mode(sliding_sync).await?;
                    Recovering
                } else {
                    Running
                }
            }

            Error { from: previous_state } | Terminated { from: previous_state } => {
                match previous_state.as_ref() {
                    // Unreachable state.
                    Error { .. } | Terminated { .. } => {
                        unreachable!(
                            "It's impossible to reach `Error` or `Terminated` from `Error` or `Terminated`"
                        );
                    }

                    // If the previous state was `Running`, we enter the `Recovering` state.
                    Running => {
                        set_all_rooms_to_selective_sync_mode(sliding_sync).await?;
                        Recovering
                    }

                    // Jump back to the previous state that led to this termination.
                    state => state.to_owned(),
                }
            }
        };

        Ok(next_state)
    }
}

async fn set_all_rooms_to_growing_sync_mode(sliding_sync: &SlidingSync) -> Result<(), Error> {
    sliding_sync
        .on_list(ALL_ROOMS_LIST_NAME, |list| {
            list.set_sync_mode(SlidingSyncMode::new_growing(ALL_ROOMS_DEFAULT_GROWING_BATCH_SIZE));

            ready(())
        })
        .await
        .ok_or_else(|| Error::UnknownList(ALL_ROOMS_LIST_NAME.to_owned()))
}

async fn set_all_rooms_to_selective_sync_mode(sliding_sync: &SlidingSync) -> Result<(), Error> {
    sliding_sync
        .on_list(ALL_ROOMS_LIST_NAME, |list| {
            list.set_sync_mode(
                SlidingSyncMode::new_selective().add_range(ALL_ROOMS_DEFAULT_SELECTIVE_RANGE),
            );

            ready(())
        })
        .await
        .ok_or_else(|| Error::UnknownList(ALL_ROOMS_LIST_NAME.to_owned()))
}

/// Default `batch_size` for the selective sync-mode of the
/// `ALL_ROOMS_LIST_NAME` list.
pub const ALL_ROOMS_DEFAULT_SELECTIVE_RANGE: Range = 0..=19;

/// Default `batch_size` for the growing sync-mode of the `ALL_ROOMS_LIST_NAME`
/// list.
pub const ALL_ROOMS_DEFAULT_GROWING_BATCH_SIZE: u32 = 100;

#[cfg(test)]
mod tests {
    use matrix_sdk_test::async_test;

    use super::{super::tests::new_room_list, *};

    #[async_test]
    async fn test_states() -> Result<(), Error> {
        let room_list = new_room_list().await?;
        let sliding_sync = room_list.sliding_sync();

        let state_machine = StateMachine::new();

        // First state.
        let state = State::Init;

        // Hypothetical error.
        {
            let state = state_machine
                .next(State::Error { from: Box::new(state.clone()) }, sliding_sync)
                .await?;

            // Back to the previous state.
            assert_eq!(state, State::Init);
        }

        // Hypothetical termination.
        {
            let state = state_machine
                .next(State::Terminated { from: Box::new(state.clone()) }, sliding_sync)
                .await?;

            // Back to the previous state.
            assert_eq!(state, State::Init);
        }

        // Next state.
        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::SettingUp);

        // Hypothetical error.
        {
            let state = state_machine
                .next(State::Error { from: Box::new(state.clone()) }, sliding_sync)
                .await?;

            // Back to the previous state.
            assert_eq!(state, State::SettingUp);
        }

        // Hypothetical termination.
        {
            let state = state_machine
                .next(State::Terminated { from: Box::new(state.clone()) }, sliding_sync)
                .await?;

            // Back to the previous state.
            assert_eq!(state, State::SettingUp);
        }

        // Next state.
        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::Running);

        // Hypothetical error.
        {
            let state = state_machine
                .next(State::Error { from: Box::new(state.clone()) }, sliding_sync)
                .await?;

            // Jump to the **recovering** state!
            assert_eq!(state, State::Recovering);

            let state = state_machine.next(state, sliding_sync).await?;

            // Now, back to the previous state.
            assert_eq!(state, State::Running);
        }

        // Hypothetical termination.
        {
            let state = state_machine
                .next(State::Terminated { from: Box::new(state.clone()) }, sliding_sync)
                .await?;

            // Jump to the **recovering** state!
            assert_eq!(state, State::Recovering);

            let state = state_machine.next(state, sliding_sync).await?;

            // Now, back to the previous state.
            assert_eq!(state, State::Running);
        }

        // Hypothetical error when recovering.
        {
            let state = state_machine
                .next(State::Error { from: Box::new(State::Recovering) }, sliding_sync)
                .await?;

            // Back to the previous state.
            assert_eq!(state, State::Recovering);
        }

        // Hypothetical termination when recovering.
        {
            let state = state_machine
                .next(State::Terminated { from: Box::new(State::Recovering) }, sliding_sync)
                .await?;

            // Back to the previous state.
            assert_eq!(state, State::Recovering);
        }

        Ok(())
    }

    #[async_test]
    async fn test_recover_state_after_delay() -> Result<(), Error> {
        let room_list = new_room_list().await?;
        let sliding_sync = room_list.sliding_sync();

        let mut state_machine = StateMachine::new();
        state_machine.delay_before_recover = Duration::from_millis(50);

        let state = State::Init;

        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::SettingUp);

        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::Running);

        // We haven't reach `delay_before_recover` yet so should still be running
        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::Running);

        tokio::time::sleep(Duration::from_millis(100)).await;

        // `delay_before_recover` reached, time to recover
        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::Recovering);

        let state = state_machine.next(state, sliding_sync).await?;
        assert_eq!(state, State::Running);

        Ok(())
    }

    #[async_test]
    async fn test_action_set_all_rooms_list_to_growing_and_selective_sync_mode() -> Result<(), Error>
    {
        let room_list = new_room_list().await?;
        let sliding_sync = room_list.sliding_sync();

        // List is present, in Selective mode.
        assert_eq!(
            sliding_sync
                .on_list(ALL_ROOMS_LIST_NAME, |list| ready(matches!(
                    list.sync_mode(),
                    SlidingSyncMode::Selective { ranges } if ranges == vec![ALL_ROOMS_DEFAULT_SELECTIVE_RANGE]
                )))
                .await,
            Some(true)
        );

        // Run the action!
        set_all_rooms_to_growing_sync_mode(sliding_sync).await.unwrap();

        // List is still present, in Growing mode.
        assert_eq!(
            sliding_sync
                .on_list(ALL_ROOMS_LIST_NAME, |list| ready(matches!(
                    list.sync_mode(),
                    SlidingSyncMode::Growing {
                        batch_size, ..
                    } if batch_size == ALL_ROOMS_DEFAULT_GROWING_BATCH_SIZE
                )))
                .await,
            Some(true)
        );

        // Run the other action!
        set_all_rooms_to_selective_sync_mode(sliding_sync).await.unwrap();

        // List is still present, in Selective mode.
        assert_eq!(
            sliding_sync
                .on_list(ALL_ROOMS_LIST_NAME, |list| ready(matches!(
                    list.sync_mode(),
                    SlidingSyncMode::Selective { ranges } if ranges == vec![ALL_ROOMS_DEFAULT_SELECTIVE_RANGE]
                )))
                .await,
            Some(true)
        );

        Ok(())
    }
}
