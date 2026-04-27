use clojure_rt::{implements, protocol, register_type};

// Zero-method protocol — should auto-generate MARKER.
protocol! {
    pub trait IMyMarker {}
}

register_type! { pub struct Tagged { _phantom: clojure_rt::Value } }

// Empty-body impl — should register a marker entry.
implements! {
    impl IMyMarker for Tagged {}
}

fn main() {
    // The MARKER static and id cell must exist for the protocol.
    let _ = &IMyMarker::MARKER;
    let _ = &IMyMarker::MARKER_METHOD_ID;
}
