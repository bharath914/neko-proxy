//! The SDK's part of the client API: status, devices, prompts, settings.

use crate::pb;
use flow::{BodyState, FlowRecord};
use sdk::adb::AdbStatus;
use sdk::discovery::DiscoveryStatus;
use sdk::{ConnStatus, DeviceConfig, Redaction, Sdk, Snapshot};

pub fn status(sdk: Option<&Sdk>) -> pb::SdkStatus {
    let Some(sdk) = sdk else {
        return pb::SdkStatus::default();
    };
    let s = sdk.status();
    let (adb_state, adb_devices) = match &s.adb {
        AdbStatus::Off => ("off", Vec::new()),
        AdbStatus::Unavailable => ("unavailable", Vec::new()),
        AdbStatus::Ready { devices } => ("ready", devices.clone()),
        AdbStatus::Error(_) => ("error", Vec::new()),
    };
    let (discovery_state, discovery_error) = match &s.discovery {
        DiscoveryStatus::Off => ("off", String::new()),
        DiscoveryStatus::Advertising => ("advertising", String::new()),
        DiscoveryStatus::Error(e) => ("error", e.clone()),
    };
    pb::SdkStatus {
        available: true,
        enabled: s.enabled,
        port: s.port as u32,
        fingerprint: s.fingerprint,
        desktop_name: s.desktop_name,
        listening: s.listening,
        lan_addresses: sdk.lan_addresses(),
        adb_state: adb_state.into(),
        adb_label: s.adb.label(),
        adb_devices,
        discovery_state: discovery_state.into(),
        discovery_error,
        sessions: s.sessions as u32,
        stats: Some(pb::SdkStats {
            calls: s.stats.calls,
            events: s.stats.events,
            dropped_chunks: s.stats.dropped_chunks,
            dropped_calls: s.stats.dropped_calls,
            dropped_events: s.stats.dropped_events,
            protocol_violations: s.stats.protocol_violations,
            rejected_connections: s.stats.rejected_connections,
            incomplete_calls: s.stats.incomplete_calls,
        }),
    }
}

pub fn config_to_pb(c: &DeviceConfig) -> pb::DeviceConfig {
    pb::DeviceConfig {
        capture_enabled: c.capture_enabled,
        host_allow: c.host_allow.clone(),
        host_deny: c.host_deny.clone(),
        body_cap_bytes: c.body_cap_bytes,
        eager_body_bytes: c.eager_body_bytes,
        redaction: match c.redaction {
            Redaction::Default => pb::Redaction::RedactDefault,
            Redaction::Strict => pb::Redaction::RedactStrict,
            Redaction::None => pb::Redaction::RedactNone,
        } as i32,
        extra_redact_keys: c.extra_redact_keys.clone(),
    }
}

pub fn config_from_pb(c: &pb::DeviceConfig) -> DeviceConfig {
    DeviceConfig {
        capture_enabled: c.capture_enabled,
        host_allow: c.host_allow.clone(),
        host_deny: c.host_deny.clone(),
        body_cap_bytes: c.body_cap_bytes,
        eager_body_bytes: c.eager_body_bytes,
        redaction: match pb::Redaction::try_from(c.redaction)
            .unwrap_or(pb::Redaction::RedactDefault)
        {
            pb::Redaction::RedactDefault => Redaction::Default,
            pb::Redaction::RedactStrict => Redaction::Strict,
            pb::Redaction::RedactNone => Redaction::None,
        },
        extra_redact_keys: c.extra_redact_keys.clone(),
    }
    .sanitized()
}

pub fn snapshot(s: &Snapshot) -> pb::DevicesSnapshot {
    pb::DevicesSnapshot {
        rev: s.rev,
        enabled: s.enabled,
        devices: s
            .devices
            .iter()
            .map(|d| pb::Device {
                device_id: d.facts.device_id.clone(),
                app_id: d.facts.app_id.clone(),
                device_name: d.facts.device_name.clone(),
                device_model: d.facts.device_model.clone(),
                os_version: d.facts.os_version.clone(),
                app_name: d.facts.app_name.clone(),
                app_version: d.facts.app_version.clone(),
                sdk_name: d.facts.sdk_name.clone(),
                sdk_version: d.facts.sdk_version.clone(),
                status: match d.status {
                    ConnStatus::Idle => pb::DeviceStatus::DeviceIdle,
                    ConnStatus::Connected => pb::DeviceStatus::DeviceConnected,
                    ConnStatus::Denied => pb::DeviceStatus::DeviceDenied,
                    ConnStatus::Revoked => pb::DeviceStatus::DeviceRevoked,
                } as i32,
                connection: d.connection.clone(),
                remote_ip: d.remote_ip.clone(),
                last_seen_ms: d.last_seen_ms,
                flows: d.flows,
                dropped: d.dropped,
                config: Some(config_to_pb(&d.config)),
            })
            .collect(),
        pending: s
            .pending
            .iter()
            .map(|p| pb::PendingPrompt {
                id: p.id,
                device_id: p.device_id.clone(),
                app_id: p.app_id.clone(),
                device_name: p.device_name.clone(),
                device_model: p.device_model.clone(),
                os_version: p.os_version.clone(),
                app_name: p.app_name.clone(),
                sdk_name: p.sdk_name.clone(),
                sdk_version: p.sdk_version.clone(),
                connection: p.connection.clone(),
                remote_ip: p.remote_ip.clone(),
                code: p.code.clone(),
                created_ms: p.created_ms,
                expires_ms: p.expires_ms,
                repair: p.repair,
            })
            .collect(),
    }
}

fn body_state(s: BodyState) -> pb::BodyState {
    match s {
        BodyState::Present => pb::BodyState::BodyPresent,
        BodyState::Lazy => pb::BodyState::BodyLazy,
        BodyState::Skipped => pb::BodyState::BodySkipped,
        BodyState::Gone => pb::BodyState::BodyGone,
    }
}

pub fn context(rec: &FlowRecord) -> Option<pb::SdkContext> {
    let i = rec.sdk.as_deref()?;
    Some(pb::SdkContext {
        device_id: i.device_id.clone(),
        app_id: i.app_id.clone(),
        device_name: i.device_name.clone(),
        device_model: i.device_model.clone(),
        os_version: i.os_version.clone(),
        app_name: i.app_name.clone(),
        app_version: i.app_version.clone(),
        build: i.build.clone(),
        sdk_name: i.sdk_name.clone(),
        sdk_version: i.sdk_version.clone(),
        connection: i.connection.clone(),
        name: i.name.clone(),
        owner: i.owner.clone(),
        tag: i.tag.clone(),
        screen: i.screen.clone(),
        route_template: i.route_template.clone(),
        client_stack: i.client_stack.clone(),
        from_cache: i.from_cache,
        request_redacted: i.req_redacted,
        response_redacted: i.res_redacted,
        request_body_state: body_state(i.req_body) as i32,
        response_body_state: body_state(i.res_body) as i32,
        timings: i.timings.map(|t| {
            let f = |v: i32| v as f64;
            pb::Timings {
                dns_ms: f(t.dns),
                connect_ms: f(t.connect),
                tls_ms: f(t.tls),
                send_ms: f(t.request_sent),
                wait_ms: f(t.ttfb),
                receive_ms: f(t.download),
                total_ms: rec.timings.total_us as f64 / 1000.0,
                reused_connection: rec.timings.reused,
            }
        }),
    })
}
