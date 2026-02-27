//! `Pipeline` orchestrator — capture loop → ROI → consumer fan-out → metrics → triggers.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod consumer;
pub mod metrics;
pub mod recorder;
pub mod ring_buffer;

use crate::capture::CaptureBackend;
use crate::config::ProfileConfig;
use crate::hooks::InputHookBackend;
use crate::pipeline::consumer::{ConsumerError, FrameConsumer};
use crate::pipeline::metrics::PipelineMetrics;
use crate::roi::RoiManager;
use crate::session::triggers::{TerminationTrigger, TriggerState};
use std::sync::Arc;

/// Result type for pipeline operations.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Capture error: {0}")]
    Capture(#[from] crate::capture::CaptureError),
    #[error("Consumer error: {0}")]
    Consumer(#[from] ConsumerError),
    #[error("Hook error: {0}")]
    Hook(#[from] crate::hooks::HookError),
}

/// Reason the pipeline run ended.
#[derive(Debug)]
pub enum PipelineStop {
    /// A trigger fired with the given reason.
    TriggerFired(String),
    /// An unrecoverable error occurred.
    Error(PipelineError),
}

/// Main pipeline orchestrator.
///
/// Runs the capture loop, evaluates ROI each frame, fans out to consumers,
/// updates metrics, and polls termination triggers.
#[allow(dead_code)]
pub struct Pipeline {
    config: ProfileConfig,
    backend: Box<dyn CaptureBackend>,
    hook: Box<dyn InputHookBackend>,
    consumers: Vec<Box<dyn FrameConsumer>>,
    triggers: Vec<Box<dyn TerminationTrigger>>,
    roi_manager: RoiManager,
    metrics: Arc<PipelineMetrics>,
}

impl Pipeline {
    pub fn new(
        config: ProfileConfig,
        backend: Box<dyn CaptureBackend>,
        hook: Box<dyn InputHookBackend>,
        consumers: Vec<Box<dyn FrameConsumer>>,
        triggers: Vec<Box<dyn TerminationTrigger>>,
        roi_manager: RoiManager,
        metrics: Arc<PipelineMetrics>,
    ) -> Self {
        Self {
            config,
            backend,
            hook,
            consumers,
            triggers,
            roi_manager,
            metrics,
        }
    }

    /// Run the pipeline until a trigger fires or an error occurs.
    ///
    /// Returns the stop reason.
    pub fn run(&mut self) -> PipelineStop {
        let mut frame_id: u64 = 0;
        let _gap_threshold_ns: u64 = 66_000_000; // 66 ms → gap flag

        loop {
            // Acquire frame.
            let frame = match self.backend.acquire_frame() {
                Ok(Some(f)) => f,
                Ok(None) => continue, // no frame yet (timeout)
                Err(e) => return PipelineStop::Error(PipelineError::Capture(e)),
            };

            // Drain hook events since last frame.
            let mut events = Vec::new();
            while let Some(ev) = self.hook.try_recv() {
                events.push(ev);
            }

            // ROI evaluation.
            self.roi_manager.update(&frame);

            // Gap detection.
            if frame_id > 0 {
                // TODO(T033): compare with previous frame timestamp.
            }

            // Consumer fan-out.
            for consumer in &mut self.consumers {
                if let Err(e) = consumer.consume(&frame, frame_id) {
                    // Log and continue in degraded mode (Principle IV).
                    eprintln!("[WARN] Consumer error on frame {}: {}", frame_id, e);
                }
            }

            // Trigger polling.
            for trigger in &mut self.triggers {
                match trigger.check(&frame, frame_id, &events) {
                    TriggerState::Terminate { reason, .. } => {
                        // Flush all consumers before returning.
                        for consumer in &mut self.consumers {
                            let _ = consumer.flush();
                        }
                        return PipelineStop::TriggerFired(reason);
                    }
                    TriggerState::Continue => {}
                }
            }

            // Release frame.
            if let Err(e) = self.backend.release_frame(frame) {
                return PipelineStop::Error(PipelineError::Capture(e));
            }

            frame_id += 1;
        }
    }
}
