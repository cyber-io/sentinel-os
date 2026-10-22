//! Event handling for TUI

use std::time::Duration;
use crossterm::event::{self, Event};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub struct EventHandler {
    rx: UnboundedReceiverStream<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn event loop
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(tick_rate));
            loop {
                interval.tick().await;
                if let Ok(true) = event::poll(Duration::from_millis(0)) {
                    if let Ok(event) = event::read() {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self {
            rx: UnboundedReceiverStream::new(rx),
            _tx: tx,
        }
    }

    pub async fn next(&mut self) -> anyhow::Result<Event> {
        self.rx.next().await.ok_or_else(|| anyhow::anyhow!("Event stream ended"))
    }
}