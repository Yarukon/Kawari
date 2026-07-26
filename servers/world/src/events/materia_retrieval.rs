use std::time::Duration;

use async_trait::async_trait;
use kawari::ipc::zone::Condition;

use crate::{DeferredTask, Event, EventHandler, ZoneConnection, lua::LuaPlayer};

/// How long the retrieval animation runs before the result may be sent.
///
/// Measured from a retail capture: EventAction1 at 20.612, result burst at 23.061.
const RETRIEVAL_ANIMATION_TIME: Duration = Duration::from_millis(2450);

/// For materia retrieval (拆) events, opened by ClientTrigger 2800.
///
/// The retrieval itself runs from the retrieval action cast inside this event, not from here --
/// this handler exists so the event can be opened at all. Without it `dispatch_event` returns
/// `None`, no condition is ever set or cleared, and the client hangs waiting for an event that
/// never starts.
#[derive(Debug)]
pub struct MateriaRetrievalEventHandler;

impl Default for MateriaRetrievalEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MateriaRetrievalEventHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl EventHandler for MateriaRetrievalEventHandler {
    async fn on_yield(
        &self,
        _event: &Event,
        connection: &mut ZoneConnection,
        _scene: u16,
        _action_id: u8,
        _params: &[i32],
        _player: &mut LuaPlayer,
    ) {
        // The client fires EventAction1 the instant it casts the retrieval action -- captures show
        // it arriving ~1ms after ActionSend -- and then plays the retrieval animation. Retail does
        // not answer until roughly 2.45s later, right before the client sends EventFinish1.
        //
        // That gap is the animation, and it is load-bearing: answering immediately makes the client
        // apply the result and cut its own animation short, so the retrieval visibly snaps.
        connection.schedule_task(
            DeferredTask::FinishMateriaRetrieval,
            RETRIEVAL_ANIMATION_TIME,
        );
    }

    async fn on_return(
        &self,
        _event: &Event,
        _connection: &mut ZoneConnection,
        _scene: u16,
        _results: &[i32],
        player: &mut LuaPlayer,
    ) {
        player.finish_event();
    }

    /// Captures show condition bit 14 (`0x4000`) set for the duration of the retrieval event.
    fn condition(&self) -> Condition {
        Condition::Occupied39
    }
}
