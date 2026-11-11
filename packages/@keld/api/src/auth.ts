/** Host-owned auth over the application's one canonical link (macOS T3). */
import { kipcError, linkError, requireDeadline } from "@keld/kipc";
import { invokeAuth } from "./app.ts";
import {
  AUTH_BEGIN_CALL_DEADLINE_MS,
  AUTH_TOKEN_CALL_DEADLINE_MS,
  decodeAuthResponse,
  encodeAuthRequest,
  type AuthRequest,
  type AuthResponse,
} from "./auth.generated.ts";

type NativeBegin = Extract<AuthRequest, { variant: "Begin" }>;
type NativeToken = Extract<AuthRequest, { variant: "Token" }>;
type NativeBeginResult = Extract<AuthResponse, { variant: "Begin" }>;
type NativeTokenResult = Extract<AuthResponse, { variant: "Token" }>;

/** JS request shape; the generated codec alone owns Rust snake_case wire fields. */
export interface AuthBeginRequest {
  provider: NativeBegin["provider"];
  scopes?: NativeBegin["scopes"];
  loginHint?: NativeBegin["login_hint"];
}

/** An opaque host-session account selector and optional requested OAuth scopes. */
export interface AuthTokenRequest {
  accountId: NativeToken["account_id"];
  scopes?: NativeToken["scopes"];
}

/** Result of a successful interactive flow. */
export interface AuthBeginResult {
  accountId: NativeBeginResult["account_id"];
  grantedScopes: NativeBeginResult["granted_scopes"];
}

/** Access token result; expiresAt is exact whole UTC UNIX seconds as bigint. */
export interface AuthTokenResult {
  accessToken: NativeTokenResult["access_token"];
  expiresAt: NativeTokenResult["expires_at"];
  tokenType: NativeTokenResult["token_type"];
}

/** Override the finite reply wait through the existing WorkerLink call API. */
export interface AuthCallOptions {
  deadlineMs?: number;
}

function requireRequest(value: unknown): void {
  if (!value || typeof value !== "object") {
    throw kipcError("KELD-IPC-003", "auth request must be an object");
  }
}

function callerDeadline(
  options: AuthCallOptions | undefined,
  fallback: number,
): number {
  if (
    options !== undefined &&
    (options === null || typeof options !== "object" || Array.isArray(options))
  ) {
    throw linkError(
      "KELD-IPC-005",
      "auth call options must be an object. Pass an object with a finite deadlineMs, or omit options.",
    );
  }
  // Read the caller-owned property once. Only missing/undefined selects a default.
  const supplied = options?.deadlineMs;
  const deadlineMs = supplied === undefined ? fallback : supplied;
  requireDeadline(deadlineMs);
  return deadlineMs;
}

/**
 * Privileged authentication supplied by the macOS T3 host backend.
 *
 * Calls use the actual Core auth channel and reject with its typed errors;
 * this API adds no local success path or unsupported-platform fallback.
 * No automatic retry occurs. Caller deadlines start with the outbound call,
 * independently of app.whenReady and the host's admitted flow deadline.
 */
export const auth: {
  begin(
    request: AuthBeginRequest,
    options?: AuthCallOptions,
  ): Promise<AuthBeginResult>;
  token(
    request: AuthTokenRequest,
    options?: AuthCallOptions,
  ): Promise<AuthTokenResult>;
} = {
  async begin(request, options): Promise<AuthBeginResult> {
    const deadlineMs = callerDeadline(options, AUTH_BEGIN_CALL_DEADLINE_MS);
    requireRequest(request);
    // The generated encoder snapshots scopes before readiness can yield.
    const payload = encodeAuthRequest({
      variant: "Begin",
      provider: request.provider,
      scopes: request.scopes ?? null,
      login_hint: request.loginHint ?? null,
    });
    const response = decodeAuthResponse(await invokeAuth(payload, deadlineMs));
    if (response.variant !== "Begin")
      throw kipcError("KELD-IPC-003", "auth.begin received a Token response");
    return {
      accountId: response.account_id,
      grantedScopes: response.granted_scopes,
    };
  },

  async token(request, options): Promise<AuthTokenResult> {
    const deadlineMs = callerDeadline(options, AUTH_TOKEN_CALL_DEADLINE_MS);
    requireRequest(request);
    const payload = encodeAuthRequest({
      variant: "Token",
      account_id: request.accountId,
      scopes: request.scopes ?? null,
    });
    const response = decodeAuthResponse(await invokeAuth(payload, deadlineMs));
    if (response.variant !== "Token")
      throw kipcError("KELD-IPC-003", "auth.token received a Begin response");
    return {
      accessToken: response.access_token,
      expiresAt: response.expires_at,
      tokenType: response.token_type,
    };
  },
};
