use serde::Serialize;
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
    Manual,
    AiReadOnly,
    AiAssist,
    AiAutonomous,
    AutomationPaused,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelActor {
    Human,
    Ai,
    System,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caller {
    pub actor: ChannelActor,
    pub trusted: bool,
}
impl Caller {
    pub const TRUSTED_HUMAN: Self = Self {
        actor: ChannelActor::Human,
        trusted: true,
    };
    pub const AI: Self = Self {
        actor: ChannelActor::Ai,
        trusted: false,
    };
    pub const SYSTEM: Self = Self {
        actor: ChannelActor::System,
        trusted: true,
    };
}
impl ControlMode {
    pub fn ai_can_write(self) -> bool {
        self == Self::AiAutonomous
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ControlStatus {
    pub mode: ControlMode,
    pub can_ai_write: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_mode: Option<ControlMode>,
}
struct State {
    mode: ControlMode,
    previous: Option<ControlMode>,
}
#[derive(Clone)]
pub struct ControlState {
    state: Arc<RwLock<State>>,
    gate: Arc<RwLock<()>>,
}
impl Default for ControlState {
    fn default() -> Self {
        Self::new(ControlMode::Manual)
    }
}
impl ControlState {
    pub fn new(mode: ControlMode) -> Self {
        Self {
            state: Arc::new(RwLock::new(State {
                mode,
                previous: None,
            })),
            gate: Arc::new(RwLock::new(())),
        }
    }
    pub fn mode(&self) -> ControlMode {
        self.state.read().expect("control poisoned").mode
    }
    pub fn status(&self) -> ControlStatus {
        let s = self.state.read().expect("control poisoned");
        ControlStatus {
            mode: s.mode,
            can_ai_write: s.mode.ai_can_write(),
            previous_mode: s.previous,
        }
    }
    pub fn write_guard(&self) -> Result<std::sync::RwLockReadGuard<'_, ()>, String> {
        self.gate.read().map_err(|_| "control gate poisoned".into())
    }
    pub fn ensure_ai_write(&self) -> Result<(), String> {
        if self.mode().ai_can_write() {
            Ok(())
        } else {
            Err("AI write requires ai_autonomous".into())
        }
    }
    pub fn ensure_human_write(&self) -> Result<(), String> {
        if self.mode() == ControlMode::Manual {
            Ok(())
        } else {
            Err("human write requires manual mode; use takeover_manual".into())
        }
    }
    pub fn handoff(&self, caller: Caller, mode: ControlMode) -> Result<ControlStatus, String> {
        if caller.actor != ChannelActor::Human || !caller.trusted {
            return Err("handoff requires trusted human".into());
        }
        if !matches!(
            mode,
            ControlMode::AiReadOnly | ControlMode::AiAssist | ControlMode::AiAutonomous
        ) {
            return Err("invalid AI handoff mode".into());
        }
        let _g = self
            .gate
            .write()
            .map_err(|_| "control gate poisoned".to_string())?;
        let mut s = self
            .state
            .write()
            .map_err(|_| "control poisoned".to_string())?;
        s.mode = mode;
        s.previous = None;
        Ok(ControlStatus {
            mode,
            can_ai_write: mode.ai_can_write(),
            previous_mode: None,
        })
    }
    pub fn pause_automation(&self, caller: Caller) -> Result<ControlStatus, String> {
        if caller.actor != ChannelActor::Human || !caller.trusted {
            return Err("pause automation requires trusted human".into());
        }
        let _g = self
            .gate
            .write()
            .map_err(|_| "control gate poisoned".to_string())?;
        let mut s = self
            .state
            .write()
            .map_err(|_| "control poisoned".to_string())?;
        if s.mode != ControlMode::AutomationPaused {
            s.previous = Some(s.mode)
        }
        s.mode = ControlMode::AutomationPaused;
        Ok(ControlStatus {
            mode: s.mode,
            can_ai_write: false,
            previous_mode: s.previous,
        })
    }
    pub fn begin_takeover(
        &self,
        caller: Caller,
    ) -> Result<std::sync::RwLockWriteGuard<'_, ()>, String> {
        if caller.actor != ChannelActor::Human || !caller.trusted {
            return Err("takeover requires trusted human".into());
        }
        let g = self
            .gate
            .write()
            .map_err(|_| "control gate poisoned".to_string())?;
        let mut s = self
            .state
            .write()
            .map_err(|_| "control poisoned".to_string())?;
        let old = s.mode;
        s.mode = ControlMode::Manual;
        s.previous = Some(old);
        Ok(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ai_cannot_handoff_or_write() {
        let c = ControlState::default();
        assert!(c.handoff(Caller::AI, ControlMode::AiAutonomous).is_err());
        assert!(c.ensure_ai_write().is_err());
    }
    #[test]
    fn trusted_handoff_then_manual_gate() {
        let c = ControlState::default();
        c.handoff(Caller::TRUSTED_HUMAN, ControlMode::AiAutonomous)
            .unwrap();
        assert!(c.ensure_ai_write().is_ok());
        let _g = c.begin_takeover(Caller::TRUSTED_HUMAN).unwrap();
        assert_eq!(c.mode(), ControlMode::Manual);
        assert!(c.ensure_ai_write().is_err());
    }
}
