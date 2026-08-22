# Virtual port API (`keld_runtime::registry`)

Status: KEL-75 T3 · Unix authenticated roles only

## Overview

The host owns every virtual port pair. Roles never connect directly; the host
mediates send, one-shot transfer, close, and generation revocation.

## Types

| Type | Role |
|---|---|
| `RolePrincipal` | Host-minted `(RoleOwner, RoleGeneration)` identity |
| `PortCapability` | Reference to one live end of a pair |
| `PortMessage` | Inline bounded payload (`MAX_PORT_MESSAGE_LEN` = 4096) |
| `VirtualPortRegistry` | Host-owned pair table and route state |

## Operations

### Create pair

```rust
let primary = RolePrincipal::new(RoleOwner::Primary, generation);
let app = RolePrincipal::new(RoleOwner::AppBound, generation);
let (cap_a, cap_b) = registry.create_role_port_pair(primary, app)?;
```

Both principals must be live generations not previously revoked in the
registry.

### Send (FIFO)

```rust
registry.virtual_ports_mut().send(cap_a, primary, b"hello")?;
let msg = registry.virtual_ports_mut().recv(cap_b, app)?.expect("one message");
```

Queue capacity defaults to 64 messages per end (`DEFAULT_PORT_QUEUE_CAPACITY`).
Overflow returns `KELD-RUNTIME-010`.

### Transfer (one-shot)

```rust
registry.virtual_ports_mut().transfer(cap_b, app, target_principal)?;
```

Failures: self (`007`), duplicate (`008`), source after relinquish (`009`),
closed (`006`), stale generation (`005`).

### Close and disconnect

```rust
registry.virtual_ports_mut().close(cap_a, primary)?;
let reason = registry
    .virtual_ports_mut()
    .poll_disconnect(cap_b, app)?
    .expect("exactly one peer disconnect");
```

`poll_disconnect` returns `Some` exactly once per end generation.

### Revoke generation

```rust
registry.revoke_role_ports(principal);
```

Invalidates every pair touching that generation.

## Errors (`KELD-RUNTIME-004`–`011`)

| Code | When |
|---|---|
| 004 | Wrong owner / foreign delivery |
| 005 | Stale role generation |
| 006 | Closed or revoked pair |
| 007 | Self transfer |
| 008 | Duplicate transfer |
| 009 | Source transfer after relinquish |
| 010 | Queue full |
| 011 | Message too large |

## Auth

No default-deny bypass. Port capabilities are host-minted; children receive
capabilities only through future facade layers, not raw OS handles.
