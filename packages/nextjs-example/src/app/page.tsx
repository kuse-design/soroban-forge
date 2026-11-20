"use client";

import {
  type ChangeEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Client,
  networks,
  type EscrowData,
} from "@soroban-forge/escrow-client";
import {
  DEFAULT_DEMO_AMOUNT,
  DEFAULT_DEMO_TIMEOUT_SECS,
  DEFAULT_DEMO_TOKEN,
  DEMO_STATE_SCHEMA,
  DEMO_STATE_STORAGE_KEY,
  DEMO_STATE_VERSION,
  DemoStateValidationError,
  browserDemoStorage,
  emptyDemoState,
  importDemoState,
  loadDemoState,
  resetDemoState,
  saveDemoState,
  serializeDemoStateExport,
  validateDemoState,
  type DemoState,
} from "../lib/demo-state";

declare global {
  interface Window {
    freighterApi?: {
      isConnected(): Promise<boolean>;
      getPublicKey(): Promise<string>;
      signTransaction(
        xdr: string,
        opts?: { networkPassphrase?: string; address?: string }
      ): Promise<{ signedTxXdr: string }>;
    };
  }
}

const RPC_URL = "https://soroban-testnet.stellar.org";
const NETWORK_PASSPHRASE = networks.testnet.networkPassphrase;
const CONTRACT_ID = networks.testnet.contractId;

// 1 unit = 10^7 stroops (SEP-41 default for 7-decimal tokens such as XLM)
const DECIMALS = BigInt(7);
const SCALE = BigInt(10) ** DECIMALS;

type LogEntry = { id: number; time: string; text: string; kind: "info" | "ok" | "err" };

let logId = 0;

function ts(): string {
  return new Date().toLocaleTimeString();
}

export default function Home() {
  const [account, setAccount] = useState<string | null>(null);
  const [client, setClient] = useState<Client | null>(null);
  const [busy, setBusy] = useState(false);
  const [log, setLog] = useState<LogEntry[]>([]);
  const [friendbotBusy, setFriendbotBusy] = useState(false);

  // Create-escrow form
  const [buyer, setBuyer] = useState("");
  const [seller, setSeller] = useState("");
  const [arbiter, setArbiter] = useState("");
  const [token, setToken] = useState(DEFAULT_DEMO_TOKEN);
  const [amount, setAmount] = useState(DEFAULT_DEMO_AMOUNT);
  const [timeoutSecs, setTimeoutSecs] = useState(DEFAULT_DEMO_TIMEOUT_SECS);
  const [creating, setCreating] = useState(false);
  const [createdId, setCreatedId] = useState<string | null>(null);

  // Actions panel
  const [actionEscrowId, setActionEscrowId] = useState("");
  const [actionInFlight, setActionInFlight] = useState<string | null>(null);
  const [resolveFavor, setResolveFavor] = useState<"seller" | "buyer">(
    "seller"
  );

  // Read panel
  const [readEscrowId, setReadEscrowId] = useState("");
  const [escrow, setEscrow] = useState<EscrowData | null>(null);
  const [readBusy, setReadBusy] = useState(false);

  // List panel
  const [myIds, setMyIds] = useState<Array<bigint>>([]);
  const [listTotal, setListTotal] = useState<number | null>(null);
  const [listBusy, setListBusy] = useState(false);

  const demoHydrated = useRef(false);
  const suppressNextPersist = useRef(false);
  const cancelResetRef = useRef<HTMLButtonElement>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);

  const pushLog = useCallback(
    (text: string, kind: LogEntry["kind"] = "info") => {
      setLog((prev) => [{ id: logId++, time: ts(), text, kind }, ...prev]);
    },
    []
  );

  const applyDemoState = useCallback((state: DemoState) => {
    setBuyer(state.buyer);
    setSeller(state.seller);
    setArbiter(state.arbiter);
    setToken(state.token);
    setAmount(state.amount);
    setTimeoutSecs(state.timeoutSecs);
    setResolveFavor(state.resolveFavor);
  }, []);

  const currentDemoState = useCallback(
    (): DemoState => ({
      buyer,
      seller,
      arbiter,
      token,
      amount,
      timeoutSecs,
      resolveFavor,
    }),
    [
      buyer,
      seller,
      arbiter,
      token,
      amount,
      timeoutSecs,
      resolveFavor,
    ]
  );

  const demoStatePreview = useMemo(
    () => JSON.stringify(currentDemoState(), null, 2),
    [currentDemoState]
  );

  const persistIssue = useMemo(() => {
    try {
      validateDemoState(currentDemoState());
      return null;
    } catch (error) {
      return error instanceof DemoStateValidationError
        ? `${error.code}: ${error.message}`
        : (error as Error).message;
    }
  }, [currentDemoState]);

  useEffect(() => {
    const storage = browserDemoStorage();
    queueMicrotask(() => {
      try {
        const stored = loadDemoState(storage);
        if (stored !== null) applyDemoState(stored);
      } catch (error) {
        resetDemoState(storage);
        pushLog(
          `Stored demo state was invalid and was reset: ${(error as Error).message}`,
          "err"
        );
      } finally {
        demoHydrated.current = true;
      }
    });
  }, [applyDemoState, pushLog]);

  useEffect(() => {
    if (!demoHydrated.current) return;
    if (suppressNextPersist.current) {
      suppressNextPersist.current = false;
      return;
    }
    if (persistIssue) return;
    saveDemoState(browserDemoStorage(), currentDemoState());
  }, [currentDemoState, persistIssue]);

  useEffect(() => {
    if (!confirmReset) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setConfirmReset(false);
    };
    window.addEventListener("keydown", onKeyDown);
    cancelResetRef.current?.focus();
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [confirmReset]);

  const exportDemoState = useCallback(() => {
    try {
      const json = serializeDemoStateExport(currentDemoState());
      setImportError(null);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "soroban-forge-local-demo-state.json";
      document.body.appendChild(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 0);
      pushLog("Exported local demo state as JSON (browser only).", "ok");
    } catch (error) {
      pushLog(`Export failed: ${(error as Error).message}`, "err");
    }
  }, [currentDemoState, pushLog]);

  const handleDemoImport = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      if (!file) return;

      try {
        const raw = await file.text();
        const state = importDemoState(browserDemoStorage(), raw);
        const next = state ?? emptyDemoState();
        suppressNextPersist.current =
          JSON.stringify(next) !== demoStatePreview;
        applyDemoState(next);
        setImportError(null);
        pushLog(
          state === null
            ? "Imported empty local demo state."
            : "Imported and validated local demo state.",
          "ok"
        );
      } catch (error) {
        const message =
          error instanceof DemoStateValidationError
            ? `${error.code}: ${error.message}`
            : (error as Error).message;
        setImportError(message);
        pushLog(`Import rejected: ${message}`, "err");
      }
    },
    [applyDemoState, demoStatePreview, pushLog]
  );

  const confirmDemoReset = useCallback(() => {
    const next = emptyDemoState();
    resetDemoState(browserDemoStorage());
    suppressNextPersist.current =
      JSON.stringify(next) !== demoStatePreview;
    applyDemoState(next);
    setConfirmReset(false);
    setImportError(null);
    pushLog(
      "Reset local demo state. Wallet connection and network settings were unchanged.",
      "ok"
    );
  }, [applyDemoState, demoStatePreview, pushLog]);

  const freighterMissing =
    typeof window !== "undefined" && !window.freighterApi;

  const listMine = useCallback(
    async (c: Client, addr: string) => {
      setListBusy(true);
      try {
        const { result } = await c.escrows_for_participant({
          participant: addr,
          cursor: 0,
          limit: 10,
        });
        setMyIds(result.ids);
        setListTotal(result.total);
        pushLog(
          `escrows_for_participant(${addr.slice(0, 6)}…): ${result.ids.length} ids (total ${result.total})`,
          "ok"
        );
      } catch (e) {
        pushLog(`List escrows failed: ${(e as Error).message}`, "err");
      } finally {
        setListBusy(false);
      }
    },
    [pushLog]
  );

  const connect = useCallback(async () => {
    setBusy(true);
    try {
      if (typeof window === "undefined" || !window.freighterApi) {
        pushLog(
          "Freighter not detected. Install the Freighter wallet extension and reload.",
          "err"
        );
        return;
      }
      if (!(await window.freighterApi.isConnected())) {
        pushLog("Freighter is not connected yet. Allow access in the popup.", "err");
      }
      const pubkey = await window.freighterApi.getPublicKey();
      setAccount(pubkey);
      const c = new Client({
        rpcUrl: RPC_URL,
        networkPassphrase: NETWORK_PASSPHRASE,
        contractId: CONTRACT_ID,
        publicKey: pubkey,
        signTransaction: (xdr: string, opts?: { networkPassphrase?: string; address?: string }) =>
          window.freighterApi!.signTransaction(xdr, {
            networkPassphrase: opts?.networkPassphrase ?? NETWORK_PASSPHRASE,
            address: opts?.address ?? pubkey,
          }),
      });
      setClient(c);
      pushLog(
        `Connected to ${pubkey.slice(0, 6)}…${pubkey.slice(-4)} (testnet)`,
        "ok"
      );
      void listMine(c, pubkey);
    } catch (e) {
      pushLog(`Wallet connect failed: ${(e as Error).message}`, "err");
    } finally {
      setBusy(false);
    }
  }, [pushLog, listMine]);

  const disconnect = useCallback(() => {
    setAccount(null);
    setClient(null);
    setEscrow(null);
    setMyIds([]);
    setListTotal(null);
    setCreatedId(null);
    pushLog("Disconnected. Wallet state cleared.", "info");
  }, [pushLog]);

  const fundFriendbot = useCallback(async () => {
    if (!account) return;
    setFriendbotBusy(true);
    pushLog(`Requesting testnet funds from friendbot for ${account}…`, "info");
    try {
      const res = await fetch(`https://friendbot.stellar.org?addr=${account}`);
      const json = (await res.json()) as {
        hash?: string;
        detail?: string;
      };
      if (res.ok && json.hash) {
        pushLog(
          `Friendbot funded the account — tx hash ${json.hash}`,
          "ok"
        );
      } else {
        pushLog(`Friendbot: ${json.detail ?? "unexpected response"}`, "err");
      }
    } catch (e) {
      pushLog(`Friendbot request failed: ${(e as Error).message}`, "err");
    } finally {
      setFriendbotBusy(false);
    }
  }, [account, pushLog]);

  const createEscrow = useCallback(async () => {
    if (!client) return;
    setCreating(true);
    try {
      const amountBig = BigInt(amount || "0");
      if (amountBig <= BigInt(0)) {
        pushLog("Amount must be a positive integer number of stroops.", "err");
        return;
      }
      const timeoutBig = BigInt(timeoutSecs || "0");
      const tx = await client.create_escrow(
        {
          buyer,
          seller,
          arbiter,
          token,
          amount: amountBig,
          timeout: timeoutBig,
        },
        { timeoutInSeconds: 60 }
      );
      pushLog("create_escrow simulated — requesting signature…", "info");
      const sent = await tx.signAndSend();
      if (sent.sendTransactionResponse) {
        pushLog(
          `Submitted — tx hash ${sent.sendTransactionResponse.hash}`,
          "ok"
        );
      }
      const escrowId = sent.result.unwrap();
      setCreatedId(escrowId.toString());
      setActionEscrowId(escrowId.toString());
      setReadEscrowId(escrowId.toString());
      pushLog(`Escrow created — id ${escrowId.toString()} (Pending)`, "ok");
    } catch (e) {
      pushLog(`Create escrow failed: ${(e as Error).message}`, "err");
    } finally {
      setCreating(false);
    }
  }, [
    client,
    buyer,
    seller,
    arbiter,
    token,
    amount,
    timeoutSecs,
    pushLog,
  ]);

  const sendAction = useCallback(
    async (name: string, invoke: () => Promise<void>) => {
      setActionInFlight(name);
      try {
        pushLog(`${name} — simulating…`, "info");
        await invoke();
      } catch (e) {
        pushLog(`${name} failed: ${(e as Error).message}`, "err");
      } finally {
        setActionInFlight(null);
      }
    },
    [pushLog]
  );

  const runAction = useCallback(
    async (
      name: string,
      build: (c: Client, id: bigint) => Promise<{
        signAndSend: () => Promise<{ sendTransactionResponse?: { hash: string } }>;
      }>
    ) => {
      if (!client) return;
      const id = BigInt(actionEscrowId || "0");
      if (id <= BigInt(0)) {
        pushLog(`Enter a valid escrow id for ${name}.`, "err");
        return;
      }
      await sendAction(name, async () => {
        const tx = await build(client, id);
        pushLog(`${name} — requesting signature…`, "info");
        const sent = await tx.signAndSend();
        pushLog(
          sent.sendTransactionResponse?.hash
            ? `${name} submitted — tx hash ${sent.sendTransactionResponse.hash}`
            : `${name} succeeded.`,
          "ok"
        );
      });
    },
    [client, actionEscrowId, pushLog, sendAction]
  );

  const fetchEscrow = useCallback(
    async (id?: string) => {
      if (!client) return;
      const raw = id ?? readEscrowId;
      const escrowId = BigInt(raw || "0");
      if (escrowId <= BigInt(0)) {
        pushLog("Enter a valid escrow id to fetch.", "err");
        return;
      }
      setReadBusy(true);
      try {
        const { result } = await client.get_escrow({ escrow_id: escrowId });
        if (result.isOk()) {
          const data = result.unwrap();
          setEscrow(data);
          pushLog(
            `Fetched escrow ${escrowId.toString()} — ${data.status.tag}`,
            "ok"
          );
        } else {
          pushLog(
            `get_escrow returned error: ${result.unwrapErr().message}`,
            "err"
          );
        }
      } catch (e) {
        pushLog(`get_escrow failed: ${(e as Error).message}`, "err");
      } finally {
        setReadBusy(false);
      }
    },
    [client, readEscrowId, pushLog]
  );

  const connectProps = useMemo(
    () => ({
      disabled: busy,
      onClick: connect,
    }),
    [busy, connect]
  );

  return (
    <main className="mx-auto max-w-5xl p-6">
      <div className="mb-1 flex items-center justify-between gap-4">
        <h1 className="text-3xl font-bold">Soroban Forge — Escrow Testnet Demo</h1>
        <a href="/ui/index.html" className="shrink-0 rounded border border-gray-300 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50">
          Dashboard screens
        </a>
      </div>
      <p className="text-sm text-gray-500 mb-6">
        Contract{" "}
        <code className="break-all">{CONTRACT_ID}</code> ·{" "}
        {NETWORK_PASSPHRASE}
      </p>

      {/* Wallet */}
      <section className="rounded-xl border border-gray-300 p-4 mb-6">
        <h2 className="text-lg font-semibold mb-2">1 · Wallet</h2>
        {account ? (
          <div className="flex flex-wrap items-center gap-3">
            <span className="font-mono text-sm break-all">{account}</span>
            <span className="rounded-full bg-green-100 px-2 py-0.5 text-xs text-green-800">
              connected
            </span>
            <button
              onClick={fundFriendbot}
              disabled={friendbotBusy}
              className="rounded bg-yellow-500 px-3 py-1.5 text-sm text-white hover:bg-yellow-600 disabled:opacity-50"
            >
              {friendbotBusy ? "Funding…" : "Fund with Friendbot (testnet XLM)"}
            </button>
            <button
              onClick={disconnect}
              className="rounded bg-gray-200 px-3 py-1.5 text-sm hover:bg-gray-300"
            >
              Disconnect
            </button>
          </div>
        ) : (
          <div>
            <button
              {...(typeof window !== "undefined" && window.freighterApi
                ? connectProps
                : { disabled: true })}
              className="rounded bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700 disabled:opacity-50"
            >
              {busy ? "Connecting…" : "Connect Freighter"}
            </button>
            {freighterMissing && (
              <p className="mt-2 text-sm text-amber-700">
                Freighter not detected. Install the Freighter wallet extension
                for Stellar, create/import an account, and reload this page.
              </p>
            )}
          </div>
        )}
      </section>

      <div className="grid gap-6 lg:grid-cols-2">
        {/* Create */}
        <section className="rounded-xl border border-gray-300 p-4">
          <h2 className="text-lg font-semibold mb-3">2 · Create escrow</h2>
          <div className="space-y-2">
            <label className="block text-sm">
              Buyer (G…)
              <input
                value={buyer}
                onChange={(e) => setBuyer(e.target.value)}
                placeholder={account ?? "connected account address"}
                className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
              />
            </label>
            <label className="block text-sm">
              Seller (G…)
              <input
                value={seller}
                onChange={(e) => setSeller(e.target.value)}
                placeholder="seller public key"
                className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
              />
            </label>
            <label className="block text-sm">
              Arbiter (G…)
              <input
                value={arbiter}
                onChange={(e) => setArbiter(e.target.value)}
                placeholder="arbiter public key"
                className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
              />
            </label>
            <label className="block text-sm">
              Token (C…)
              <input
                value={token}
                onChange={(e) => setToken(e.target.value)}
                className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
              />
              <span className="text-xs text-gray-500">
                Default: repo&apos;s testnet demo SAC &quot;credit&quot; token
                (7 decimals). Replace with your own testnet token contract id.
              </span>
            </label>
            <div className="grid grid-cols-2 gap-2">
              <label className="block text-sm">
                Amount (stroops)
                <input
                  value={amount}
                  onChange={(e) => setAmount(e.target.value)}
                  placeholder="e.g. 10000000 = 1 unit"
                  className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
                />
              </label>
              <label className="block text-sm">
                Timeout (seconds)
                <input
                  value={timeoutSecs}
                  onChange={(e) => setTimeoutSecs(e.target.value)}
                  placeholder="3600"
                  className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
                />
              </label>
            </div>
            <button
              onClick={createEscrow}
              disabled={!client || creating}
              className="mt-1 w-full rounded bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700 disabled:opacity-50"
            >
              {creating
                ? "Creating… (simulate + sign + send)"
                : client
                  ? "Create escrow"
                  : "Connect wallet first"}
            </button>
            {createdId && (
              <p className="text-sm text-green-700">
                Last created escrow:{" "}
                <button
                  onClick={() => {
                    setReadEscrowId(createdId);
                    void fetchEscrow(createdId);
                  }}
                  className="font-mono underline"
                >
                  {createdId}
                </button>{" "}
                (Pending)
              </p>
            )}
          </div>
        </section>

        {/* Actions */}
        <section className="rounded-xl border border-gray-300 p-4">
          <h2 className="text-lg font-semibold mb-3">3 · Actions</h2>
          <label className="block text-sm mb-3">
            Escrow id
            <input
              value={actionEscrowId}
              onChange={(e) => setActionEscrowId(e.target.value)}
              placeholder="escrow id from create step"
              className="mt-1 w-full rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
            />
          </label>
          <div className="flex flex-wrap gap-2">
            {(["Deposit", "Release", "Refund", "Cancel", "Dispute"] as const).map(
              (name) => (
                <button
                  key={name}
                  disabled={!client || actionInFlight !== null}
                  onClick={() =>
                    void runAction(name.toLowerCase(), (c, id) =>
                      name === "Dispute"
                        ? c.dispute({ escrow_id: id, claimant: account ?? "" })
                        : c[
                            name.toLowerCase() as
                              | "deposit"
                              | "release"
                              | "refund"
                              | "cancel"
                          ]({ escrow_id: id })
                    )
                  }
                  className="rounded border border-blue-600 px-3 py-1.5 text-sm text-blue-600 hover:bg-blue-50 disabled:opacity-50"
                >
                  {actionInFlight === name.toLowerCase()
                    ? "Busy…"
                    : name}
                </button>
              )
            )}
          </div>
          <div className="mt-4 rounded border border-gray-200 p-3">
            <p className="text-sm font-medium mb-2">Resolve dispute (arbiter only)</p>
            <div className="flex items-center gap-4">
              <label className="flex items-center gap-1 text-sm">
                <input
                  type="radio"
                  name="resolve-favor"
                  checked={resolveFavor === "seller"}
                  onChange={() => setResolveFavor("seller")}
                />
                In favor of seller
              </label>
              <label className="flex items-center gap-1 text-sm">
                <input
                  type="radio"
                  name="resolve-favor"
                  checked={resolveFavor === "buyer"}
                  onChange={() => setResolveFavor("buyer")}
                />
                In favor of buyer
              </label>
              <button
                disabled={!client || actionInFlight !== null}
                onClick={() =>
                  void runAction("resolve", (c, id) =>
                    c.resolve({
                      escrow_id: id,
                      in_favor_of_seller: resolveFavor === "seller",
                    })
                  )
                }
                className="rounded bg-purple-600 px-3 py-1.5 text-sm text-white hover:bg-purple-700 disabled:opacity-50"
              >
                {actionInFlight === "resolve" ? "Busy…" : "Resolve"}
              </button>
            </div>
          </div>
        </section>
      </div>

      <div className="grid gap-6 lg:grid-cols-2 mt-6">
        {/* Read */}
        <section className="rounded-xl border border-gray-300 p-4">
          <h2 className="text-lg font-semibold mb-3">4 · Read escrow</h2>
          <div className="flex gap-2 mb-3">
            <input
              value={readEscrowId}
              onChange={(e) => setReadEscrowId(e.target.value)}
              placeholder="escrow id"
              className="flex-1 rounded border border-gray-300 px-2 py-1.5 font-mono text-sm"
            />
            <button
              onClick={() => void fetchEscrow()}
              disabled={!client || readBusy}
              className="rounded bg-gray-800 px-4 py-1.5 text-sm text-white hover:bg-gray-900 disabled:opacity-50"
            >
              {readBusy ? "Fetching…" : "Fetch"}
            </button>
          </div>
          {escrow && (
            <dl className="grid grid-cols-1 gap-x-4 gap-y-1 text-sm sm:grid-cols-2">
              <dt className="font-medium text-gray-500">Escrow id</dt>
              <dd className="font-mono">{escrow.escrow_id.toString()}</dd>
              <dt className="font-medium text-gray-500">Status</dt>
              <dd>
                <span
                  className={
                    "rounded-full px-2 py-0.5 text-xs " +
                    (escrow.status.tag === "Completed" ||
                    escrow.status.tag === "Refunded" ||
                    escrow.status.tag === "Cancelled"
                      ? "bg-gray-200 text-gray-700"
                      : escrow.status.tag === "Disputed"
                        ? "bg-red-100 text-red-700"
                        : escrow.status.tag === "Funded"
                          ? "bg-blue-100 text-blue-700"
                          : "bg-yellow-100 text-yellow-700")
                  }
                >
                  {escrow.status.tag}
                </span>
              </dd>
              <dt className="font-medium text-gray-500">Buyer</dt>
              <dd className="font-mono break-all">{escrow.buyer}</dd>
              <dt className="font-medium text-gray-500">Seller</dt>
              <dd className="font-mono break-all">{escrow.seller}</dd>
              <dt className="font-medium text-gray-500">Arbiter</dt>
              <dd className="font-mono break-all">{escrow.arbiter}</dd>
              <dt className="font-medium text-gray-500">Token</dt>
              <dd className="font-mono break-all">{escrow.token}</dd>
              <dt className="font-medium text-gray-500">Amount</dt>
              <dd className="font-mono">
                {formatAmount(escrow.amount)} ({escrow.amount.toString()} stroops)
              </dd>
              <dt className="font-medium text-gray-500">Created at</dt>
              <dd className="font-mono">
                {escrow.created_at.toString()} (
                {new Date(Number(escrow.created_at) * 1000).toLocaleString()})
              </dd>
              <dt className="font-medium text-gray-500">Timeout</dt>
              <dd className="font-mono">{escrow.timeout.toString()} s</dd>
            </dl>
          )}
        </section>

        {/* List */}
        <section className="rounded-xl border border-gray-300 p-4">
          <h2 className="text-lg font-semibold mb-3">5 · My escrows</h2>
          <div className="flex items-center gap-2">
            <button
              onClick={() => client && account && void listMine(client, account)}
              disabled={!client || listBusy}
              className="rounded bg-gray-800 px-4 py-1.5 text-sm text-white hover:bg-gray-900 disabled:opacity-50"
            >
              {listBusy ? "Loading…" : "Refresh"}
            </button>
            {listTotal !== null && (
              <span className="text-sm text-gray-500">
                {listTotal} escrow{listTotal === 1 ? "" : "s"} involving your
                account
              </span>
            )}
          </div>
          {myIds.length === 0 ? (
            <p className="mt-2 text-sm text-gray-500">
              No escrows found for the connected account yet.
            </p>
          ) : (
            <div className="mt-2 flex flex-wrap gap-2">
              {myIds.map((id) => (
                <button
                  key={id.toString()}
                  onClick={() => {
                    const s = id.toString();
                    setReadEscrowId(s);
                    void fetchEscrow(s);
                  }}
                  className="rounded-full border border-blue-600 px-3 py-1 font-mono text-sm text-blue-600 hover:bg-blue-50"
                >
                  {id.toString()}
                </button>
              ))}
            </div>
          )}
        </section>
      </div>

      <section className="mt-6 rounded-xl border border-gray-300 p-4">
        <h2 className="text-lg font-semibold mb-2">6 · Local demo state</h2>
        <p className="text-sm text-gray-600 mb-3">
          This panel covers browser-only form values. Escrow IDs produced by
          live transactions stay in the panels above and are never exported.
          Exports contain no wallet keys or signed transaction payloads.
        </p>
        <dl className="mb-3 grid gap-1 text-xs text-gray-500 sm:grid-cols-3">
          <div>
            <dt className="font-medium text-gray-500">Schema</dt>
            <dd className="break-all font-mono">{DEMO_STATE_SCHEMA}</dd>
          </div>
          <div>
            <dt className="font-medium text-gray-500">Version</dt>
            <dd className="font-mono">{DEMO_STATE_VERSION}</dd>
          </div>
          <div>
            <dt className="font-medium text-gray-500">Storage key</dt>
            <dd className="break-all font-mono">{DEMO_STATE_STORAGE_KEY}</dd>
          </div>
        </dl>
        <div className="grid gap-4 lg:grid-cols-2">
          <div>
            <h3 className="text-sm font-semibold mb-2">
              Current local form state
            </h3>
            <pre className="max-h-64 overflow-auto rounded-lg bg-gray-100 p-3 font-mono text-xs">
              {demoStatePreview}
            </pre>
          </div>
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-2">
              <button
                onClick={exportDemoState}
                className="rounded bg-blue-600 px-4 py-1.5 text-sm text-white hover:bg-blue-700"
              >
                Export local demo state
              </button>
              <button
                onClick={() => setConfirmReset(true)}
                className="rounded bg-gray-200 px-4 py-1.5 text-sm hover:bg-gray-300"
              >
                Reset local demo state
              </button>
            </div>
            <label className="block text-sm">
              Import local demo state JSON
              <input
                type="file"
                accept="application/json,.json"
                onChange={(event) => void handleDemoImport(event)}
                aria-label="Import local demo state JSON"
                className="mt-1 block w-full text-sm"
              />
            </label>
            {importError && (
              <p role="alert" className="text-sm text-red-700">
                {importError}
              </p>
            )}
            {persistIssue && (
              <p className="text-sm text-amber-700">
                Current form state is not persisted: {persistIssue}
              </p>
            )}
            <p className="text-sm text-gray-500">
              Import validates the versioned document before applying it. The
              preview shows current form values; only valid form state is
              persisted. Reset keeps your wallet connection and network settings
              unchanged.
            </p>
          </div>
        </div>
      </section>

      {confirmReset && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
          role="presentation"
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="reset-demo-state-title"
            className="w-full max-w-md rounded-xl bg-white p-5 shadow-xl"
          >
            <h2
              id="reset-demo-state-title"
              className="text-lg font-semibold mb-2"
            >
              Reset local demo state?
            </h2>
            <p className="text-sm text-gray-600 mb-4">
              This clears the browser-only form state stored by this demo. It
              does not change the network, contract, wallet connection, or any
              on-chain data.
            </p>
            <div className="flex justify-end gap-2">
              <button
                ref={cancelResetRef}
                onClick={() => setConfirmReset(false)}
                className="rounded bg-gray-200 px-4 py-1.5 text-sm hover:bg-gray-300"
              >
                Cancel
              </button>
              <button
                onClick={confirmDemoReset}
                className="rounded bg-red-600 px-4 py-1.5 text-sm text-white hover:bg-red-700"
              >
                Reset local demo state
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Log feed */}
      <section className="mt-6">
        <h2 className="text-lg font-semibold mb-2">Transaction log</h2>
        <div className="max-h-72 overflow-y-auto rounded-lg bg-gray-950 p-3 font-mono text-xs">
          {log.length === 0 ? (
            <p className="text-gray-500">No activity yet…</p>
          ) : (
            log.map((entry) => (
              <p
                key={entry.id}
                className={
                  entry.kind === "err"
                    ? "mb-1 break-all text-red-400"
                    : entry.kind === "ok"
                      ? "mb-1 break-all text-green-400"
                      : "mb-1 break-all text-gray-300"
                }
              >
                <span className="text-gray-500">[{entry.time}] </span>
                {entry.text}
              </p>
            ))
          )}
        </div>
      </section>
    </main>
  );
}

function formatAmount(stroops: bigint): string {
  const whole = stroops / SCALE;
  const frac = (stroops % SCALE).toString().padStart(Number(DECIMALS), "0");
  return `${whole.toString()}.${frac}`;
}