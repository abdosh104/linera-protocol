// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use tracing::{info_span, instrument};

#[derive(Clone)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn into_inner(self) -> Vec<u8> {
        Arc::try_unwrap(self.0)
            .expect("No other references should exist")
            .into_inner()
            .expect("Lock should not be poisoned")
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("Lock should not be poisoned")
            .write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().expect("Lock should not be poisoned").flush()
    }
}

#[instrument]
fn span_with_export() {
    tracing::info!("This span should be exported");
}

#[instrument(skip_all, fields(opentelemetry.skip = true))]
fn span_without_export() {
    tracing::info!("This span should NOT be exported");
}

#[test]
fn test_chrome_trace_includes_all_spans() {
    let buffer = SharedBuffer::new();
    let buffer_clone = buffer.clone();

    let guard = linera_base::tracing_opentelemetry::init_with_chrome_trace_exporter(
        "test_chrome_trace",
        buffer,
    );

    span_with_export();
    span_without_export();

    let manual_exported_span = info_span!("manual_exported").entered();
    tracing::info!("Manual span without skip");
    drop(manual_exported_span);

    let manual_skipped_span = info_span!("manual_skipped", opentelemetry.skip = true).entered();
    tracing::info!("Manual span with opentelemetry.skip");
    drop(manual_skipped_span);

    drop(guard);

    let trace_json = String::from_utf8(buffer_clone.into_inner()).expect("Valid UTF-8");

    assert!(
        trace_json.contains("span_with_export"),
        "Regular span should be in Chrome trace"
    );
    assert!(
        trace_json.contains("span_without_export"),
        "Chrome trace should include all spans, even those marked with opentelemetry.skip"
    );
    assert!(
        trace_json.contains("manual_exported"),
        "Manual span without skip should be in Chrome trace"
    );
    assert!(
        trace_json.contains("manual_skipped"),
        "Chrome trace should include all spans, even those marked with opentelemetry.skip"
    );
}

#[cfg(feature = "opentelemetry")]
#[test]
fn test_opentelemetry_filters_skip() {
    use tracing_subscriber::{layer::SubscriberExt as _, registry::Registry};

    let (opentelemetry_layer, exporter, tracer_provider) =
        linera_base::tracing_opentelemetry::build_opentelemetry_layer_with_test_exporter(
            "test_opentelemetry",
        );

    let subscriber = Registry::default().with(opentelemetry_layer);

    tracing::subscriber::with_default(subscriber, || {
        span_with_export();
        span_without_export();

        let manual_exported_span = info_span!("manual_exported").entered();
        tracing::info!("Manual span without skip");
        drop(manual_exported_span);

        let manual_skipped_span = info_span!("manual_skipped", opentelemetry.skip = true).entered();
        tracing::info!("Manual span with opentelemetry.skip");
        drop(manual_skipped_span);
    });

    drop(tracer_provider);

    let exported_spans = exporter
        .get_finished_spans()
        .expect("Failed to get exported spans");

    let span_names: Vec<String> = exported_spans.iter().map(|s| s.name.to_string()).collect();

    assert!(
        span_names.contains(&"span_with_export".to_string()),
        "Regular span should be exported to OpenTelemetry. Found spans: {:?}",
        span_names
    );
    assert!(
        !span_names.contains(&"span_without_export".to_string()),
        "Span with opentelemetry.skip should NOT be exported to OpenTelemetry. Found spans: {:?}",
        span_names
    );
    assert!(
        span_names.contains(&"manual_exported".to_string()),
        "Manual span without skip should be exported. Found spans: {:?}",
        span_names
    );
    assert!(
        !span_names.contains(&"manual_skipped".to_string()),
        "Manual span with opentelemetry.skip should NOT be exported. Found spans: {:?}",
        span_names
    );
}
