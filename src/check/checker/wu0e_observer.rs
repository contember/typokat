use super::wu0e_diagnostic::{DiagnosticPhase, DiagnosticPhaseKey};
use std::time::Instant;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum ObserverError {
    ActivePhase,
    NoActivePhase,
    PhaseMismatch,
    SequenceMismatch,
    ClockExhausted,
    ClockOverflow,
}

pub(super) enum ObserverClock {
    Monotonic { started: Instant },
    Fixed { ticks: [u64; 2], next: usize },
}

impl ObserverClock {
    pub(super) fn monotonic() -> Self {
        Self::Monotonic {
            started: Instant::now(),
        }
    }

    pub(super) fn fixed(ticks: [u64; 2]) -> Self {
        Self::Fixed { ticks, next: 0 }
    }

    fn read(&mut self) -> Result<u64, ObserverError> {
        match self {
            Self::Monotonic { started } => u64::try_from(started.elapsed().as_micros())
                .map_err(|_| ObserverError::ClockOverflow),
            Self::Fixed { ticks, next } => {
                let Some(value) = ticks.get(*next).copied() else {
                    return Err(ObserverError::ClockExhausted);
                };
                *next += 1;
                Ok(value)
            }
        }
    }
}

pub(super) struct DiagnosticObserver {
    clock: ObserverClock,
    entered_us: [u64; DiagnosticPhase::COUNT],
    exited_us: [u64; DiagnosticPhase::COUNT],
    states: [u8; DiagnosticPhase::COUNT],
    active: Option<(usize, DiagnosticPhaseKey)>,
    next_sequence: usize,
}

impl DiagnosticObserver {
    pub(super) fn new(clock: ObserverClock) -> Self {
        Self {
            clock,
            entered_us: [0; DiagnosticPhase::COUNT],
            exited_us: [0; DiagnosticPhase::COUNT],
            states: [0; DiagnosticPhase::COUNT],
            active: None,
            next_sequence: 0,
        }
    }

    pub(super) fn elapsed(&mut self) -> Result<u64, ObserverError> {
        self.clock.read()
    }

    pub(super) fn enter(&mut self, key: DiagnosticPhaseKey) -> Result<(usize, u64), ObserverError> {
        if self.active.is_some() {
            return Err(ObserverError::ActivePhase);
        }
        let elapsed_us = self.clock.read()?;
        let ordinal = key.phase().ordinal();
        self.entered_us[ordinal] = elapsed_us;
        self.states[ordinal] = 1;
        let sequence = self.next_sequence;
        self.active = Some((sequence, key));
        Ok((sequence, elapsed_us))
    }

    pub(super) fn exit(&mut self, key: DiagnosticPhaseKey) -> Result<(usize, u64), ObserverError> {
        let Some((sequence, active)) = self.active else {
            return Err(ObserverError::NoActivePhase);
        };
        if active != key {
            return Err(ObserverError::PhaseMismatch);
        }
        if sequence != self.next_sequence {
            return Err(ObserverError::SequenceMismatch);
        }
        let elapsed_us = self.clock.read()?;
        let ordinal = key.phase().ordinal();
        self.exited_us[ordinal] = elapsed_us;
        self.states[ordinal] = 2;
        self.active = None;
        self.next_sequence += 1;
        Ok((sequence, elapsed_us))
    }
}
