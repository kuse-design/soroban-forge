export const DEMO_STATE_STORAGE_KEY = "soroban-forge:nextjs-demo-state:v1";
export const DEMO_STATE_SCHEMA = "soroban-forge/demo-state@1";
export const DEMO_STATE_VERSION = 1;
export const DEMO_STATE_KIND = "local-demo-state";
export const DEMO_STATE_LABEL = "Local browser demo state — not on-chain";
export const DEFAULT_DEMO_TOKEN =
  "CBJQ53EOHB5MWSS7CETN523WILLNVS7NQAQQAQIS5QAYTPRCZSGKL23O";
export const DEFAULT_DEMO_AMOUNT = "10000000";
export const DEFAULT_DEMO_TIMEOUT_SECS = "3600";

export type DemoResolveFavor = "seller" | "buyer";

export interface DemoState {
  buyer: string;
  seller: string;
  arbiter: string;
  token: string;
  amount: string;
  timeoutSecs: string;
  resolveFavor: DemoResolveFavor;
}

export interface DemoStateExport {
  schema: string;
  kind: string;
  version: number;
  label: string;
  exportedAt: string;
  state: DemoState | null;
}

export type DemoStateErrorCode =
  | "invalid_json"
  | "schema_mismatch"
  | "version_mismatch"
  | "invalid_state";

export class DemoStateValidationError extends Error {
  readonly code: DemoStateErrorCode;

  constructor(code: DemoStateErrorCode, message: string) {
    super(message);
    this.name = "DemoStateValidationError";
    this.code = code;
  }
}

export interface DemoStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

const STRING_FIELDS = [
  "buyer",
  "seller",
  "arbiter",
  "token",
  "amount",
  "timeoutSecs",
] as const;

const LEGACY_FIELDS = new Set<string>([
  "createdId",
  "actionEscrowId",
  "readEscrowId",
]);

const STATE_FIELDS = new Set<string>([...STRING_FIELDS, "resolveFavor"]);

const EXPORT_FIELDS = new Set<string>([
  "schema",
  "kind",
  "version",
  "label",
  "exportedAt",
  "state",
]);

const MAX_FIELD_LENGTH = 256;

export function emptyDemoState(): DemoState {
  return {
    buyer: "",
    seller: "",
    arbiter: "",
    token: DEFAULT_DEMO_TOKEN,
    amount: DEFAULT_DEMO_AMOUNT,
    timeoutSecs: DEFAULT_DEMO_TIMEOUT_SECS,
    resolveFavor: "seller",
  };
}

export function browserDemoStorage(): DemoStorage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function validateDemoState(value: unknown): DemoState {
  if (!isRecord(value)) {
    throw new DemoStateValidationError(
      "invalid_state",
      "Demo state must be a JSON object"
    );
  }

  for (const key of Object.keys(value)) {
    if (!STATE_FIELDS.has(key) && !LEGACY_FIELDS.has(key)) {
      throw new DemoStateValidationError(
        "invalid_state",
        `Demo state contains an unexpected field: ${key}`
      );
    }
  }

  const result = emptyDemoState();

  for (const field of STRING_FIELDS) {
    const fieldValue = value[field];
    if (typeof fieldValue !== "string") {
      throw new DemoStateValidationError(
        "invalid_state",
        `Demo state field ${field} must be a string`
      );
    }
    validateStateField(field, fieldValue);
    result[field] = fieldValue;
  }

  if (value.resolveFavor !== "seller" && value.resolveFavor !== "buyer") {
    throw new DemoStateValidationError(
      "invalid_state",
      "Demo state field resolveFavor must be seller or buyer"
    );
  }
  result.resolveFavor = value.resolveFavor;

  return result;
}

export function createDemoStateExport(
  state: DemoState | null,
  now: () => Date = () => new Date()
): DemoStateExport {
  return {
    schema: DEMO_STATE_SCHEMA,
    kind: DEMO_STATE_KIND,
    version: DEMO_STATE_VERSION,
    label: DEMO_STATE_LABEL,
    exportedAt: now().toISOString(),
    state: state === null ? null : validateDemoState(state),
  };
}

export function serializeDemoStateExport(
  state: DemoState | null,
  now?: () => Date
): string {
  return `${JSON.stringify(createDemoStateExport(state, now), null, 2)}\n`;
}

export function parseDemoStateExport(raw: string): DemoState | null {
  const parsed = parseJson(raw, "Demo state export");
  return parseDemoStateExportObject(parsed);
}

export function importDemoState(
  storage: DemoStorage | null,
  raw: string
): DemoState | null {
  const state = parseDemoStateExport(raw);
  if (state === null) resetDemoState(storage);
  else saveDemoState(storage, state);
  return state;
}

export function parseStoredDemoState(raw: string): DemoState | null {
  const parsed = parseJson(raw, "Stored demo state");
  if (parsed === null) return null;
  if (!isRecord(parsed)) {
    throw new DemoStateValidationError(
      "invalid_state",
      "Stored demo state must be a JSON object or null"
    );
  }

  const isWrapped =
    "schema" in parsed || "kind" in parsed || "state" in parsed;
  return isWrapped
    ? parseDemoStateExportObject(parsed)
    : validateDemoState(parsed);
}

export function loadDemoState(storage: DemoStorage | null): DemoState | null {
  if (storage === null) return null;
  const raw = storage.getItem(DEMO_STATE_STORAGE_KEY);
  if (raw === null) return null;
  return parseStoredDemoState(raw);
}

export function saveDemoState(
  storage: DemoStorage | null,
  state: DemoState
): void {
  if (storage === null) return;
  storage.setItem(DEMO_STATE_STORAGE_KEY, serializeDemoStateExport(state));
}

export function resetDemoState(storage: DemoStorage | null): void {
  if (storage === null) return;
  storage.removeItem(DEMO_STATE_STORAGE_KEY);
}

function parseJson(raw: string, label: string): unknown {
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    throw new DemoStateValidationError(
      "invalid_json",
      `${label} is not valid JSON`
    );
  }
}

function parseDemoStateExportObject(value: unknown): DemoState | null {
  if (!isRecord(value)) {
    throw new DemoStateValidationError(
      "invalid_state",
      "Demo state export must be a JSON object"
    );
  }

  for (const key of Object.keys(value)) {
    if (!EXPORT_FIELDS.has(key)) {
      throw new DemoStateValidationError(
        "invalid_state",
        `Demo state export contains an unexpected field: ${key}`
      );
    }
  }

  if (value.schema !== DEMO_STATE_SCHEMA) {
    throw new DemoStateValidationError(
      "schema_mismatch",
      `Expected demo state schema ${DEMO_STATE_SCHEMA}`
    );
  }
  if (value.version !== DEMO_STATE_VERSION) {
    throw new DemoStateValidationError(
      "version_mismatch",
      `Unsupported demo state version: ${String(value.version)}`
    );
  }
  if (value.kind !== DEMO_STATE_KIND) {
    throw new DemoStateValidationError(
      "invalid_state",
      `Expected demo state kind ${DEMO_STATE_KIND}`
    );
  }
  if (!("state" in value)) {
    throw new DemoStateValidationError(
      "invalid_state",
      "Demo state export is missing its state"
    );
  }
  if (value.label !== undefined && typeof value.label !== "string") {
    throw new DemoStateValidationError(
      "invalid_state",
      "Demo state export label must be a string"
    );
  }
  if (value.exportedAt !== undefined) {
    if (
      typeof value.exportedAt !== "string" ||
      Number.isNaN(Date.parse(value.exportedAt))
    ) {
      throw new DemoStateValidationError(
        "invalid_state",
        "Demo state export exportedAt must be an ISO date string"
      );
    }
  }

  return value.state === null ? null : validateDemoState(value.state);
}

function validateStateField(field: string, value: string): void {
  if (value.length > MAX_FIELD_LENGTH) {
    throw new DemoStateValidationError(
      "invalid_state",
      `Demo state field ${field} exceeds ${MAX_FIELD_LENGTH} characters`
    );
  }
  if (containsSecretMaterial(value)) {
    throw new DemoStateValidationError(
      "invalid_state",
      `Demo state field ${field} contains rejected secret material`
    );
  }

  if (field === "buyer" || field === "seller" || field === "arbiter") {
    if (value !== "" && !isStellarAddress(value)) {
      throw new DemoStateValidationError(
        "invalid_state",
        `Demo state field ${field} must be empty or a valid address`
      );
    }
  } else if (field === "token") {
    if (!isContractId(value)) {
      throw new DemoStateValidationError(
        "invalid_state",
        "Demo state field token must be a valid contract id"
      );
    }
  } else if (field === "amount" || field === "timeoutSecs") {
    if (!isUnsignedInteger(value)) {
      throw new DemoStateValidationError(
        "invalid_state",
        `Demo state field ${field} must be a non-negative integer`
      );
    }
    if (field === "timeoutSecs" && /^0+$/.test(value)) {
      throw new DemoStateValidationError(
        "invalid_state",
        "Demo state field timeoutSecs must be greater than zero"
      );
    }
  }
}

function isStellarAddress(value: string): boolean {
  return /^G[2-7A-Z]{55}$/.test(value);
}

function isContractId(value: string): boolean {
  return /^C[2-7A-Z]{55}$/.test(value);
}

function isUnsignedInteger(value: string): boolean {
  return /^\d+$/.test(value);
}

function containsSecretMaterial(value: string): boolean {
  const trimmed = value.trim();
  if (/^S[2-7A-Z]{55}$/.test(trimmed)) return true;
  if (/-----BEGIN [A-Z ]*PRIVATE KEY-----/.test(value)) return true;
  return /signedTxXdr|privateKey|secretKey|mnemonic/i.test(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
