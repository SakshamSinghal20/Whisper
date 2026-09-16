(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const number = (value) =>
    Number.isFinite(value) ? new Intl.NumberFormat().format(value) : "—";
  const endpoints = {
    status: {
      method: "GET",
      path: "/api/v1/status",
      help: "Service health, chain coverage, and configured limits.",
    },
    blocks: {
      method: "GET",
      path: "/api/v1/blocks",
      help: "The most recently committed blocks in the canonical chain.",
    },
    scan: {
      method: "POST",
      path: "/api/v1/scan",
      help: "Find Taproot candidates by output prefix. Ownership verification stays local.",
    },
    transactions: {
      method: "GET",
      path: "/api/v1/transactions",
      help: "Public input tweaks and outputs for local Silent Payments scanning.",
    },
  };
  let paused = false,
    refreshing = false,
    timer,
    lastUpdate = null,
    toastTimer;
  let limits = { max_block_range: 1000, max_prefixes: 1000 };
  let nextCursor = null,
    lastRequest = null,
    responseText = "",
    sending = false;

  // Motion is local to this browser and never changes indexer polling.
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
  let wallpaperEnabled = true;
  try {
    wallpaperEnabled = localStorage.getItem("whisper.wallpaper") !== "off";
  } catch {
    // Storage can be unavailable in private or restricted browsing contexts.
  }
  function syncMotion() {
    const enabled = wallpaperEnabled && !reducedMotion.matches;
    document.body.classList.add("motion-ready");
    document.body.classList.toggle("motion-paused", !enabled);
    document.body.classList.toggle("motion-suspended", document.hidden);
    $("wallpaper-toggle").hidden = false;
    $("wallpaper-toggle").disabled = reducedMotion.matches;
    $("wallpaper-toggle").setAttribute("aria-pressed", String(enabled));
    $("wallpaper-label").textContent = reducedMotion.matches
      ? "Reduced motion"
      : "Live wallpaper";
    $("wallpaper-toggle").title = reducedMotion.matches
      ? "Animations are off to respect your system's reduced-motion preference."
      : enabled
        ? "Pause background animation"
        : "Play background animation";
  }
  $("wallpaper-toggle").addEventListener("click", () => {
    wallpaperEnabled = !wallpaperEnabled;
    try {
      localStorage.setItem(
        "whisper.wallpaper",
        wallpaperEnabled ? "on" : "off",
      );
    } catch {
      // The toggle still works for this page when preferences cannot be saved.
    }
    syncMotion();
  });
  reducedMotion.addEventListener("change", syncMotion);
  document.addEventListener("visibilitychange", syncMotion);
  syncMotion();

  function updateMetric(id, value) {
    const element = $(id);
    if (element.textContent === value) return;
    element.textContent = value;
    element.classList.remove("metric-updated");
    if (!reducedMotion.matches && wallpaperEnabled && !document.hidden) {
      // Real changes get feedback; unchanged polling results stay still.
      requestAnimationFrame(() => element.classList.add("metric-updated"));
    }
  }
  for (const metric of document.querySelectorAll(".metric strong")) {
    metric.addEventListener("animationend", () =>
      metric.classList.remove("metric-updated"),
    );
  }

  function uptime(seconds) {
    if (!Number.isFinite(seconds)) return "—";
    const days = Math.floor(seconds / 86400),
      hours = Math.floor(seconds / 3600) % 24;
    const minutes = Math.floor(seconds / 60) % 60;
    return days
      ? `${days}d ${hours}h`
      : hours
        ? `${hours}h ${minutes}m`
        : `${minutes}m ${seconds % 60}s`;
  }

  function state(id, text, tone = "") {
    $(id).textContent = text;
    $(id).classList.remove("good", "warn", "bad");
    if (tone) $(id).classList.add(tone);
  }

  function notice(title, message) {
    $("notice").hidden = !title;
    $("notice-title").textContent = title || "";
    $("notice-message").textContent = message || "";
  }

  async function fetchJSON(url, options = {}) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 15000);
    const started = performance.now();
    try {
      const response = await fetch(url, {
        ...options,
        signal: controller.signal,
        cache: "no-store",
      });
      const text = await response.text();
      let data;
      try {
        data = JSON.parse(text);
      } catch {
        data = null;
      }
      return {
        response,
        data,
        text,
        elapsed: Math.round(performance.now() - started),
      };
    } finally {
      clearTimeout(timeout);
    }
  }

  function renderStatus(result) {
    const { response, data, elapsed } = result;
    if (!data || !data.limits || !["ok", "degraded"].includes(data.status)) {
      throw new Error(
        data?.error || `The status endpoint returned HTTP ${response.status}.`,
      );
    }
    limits = data.limits;
    updateMetric("tip", number(data.tip_height));
    updateMetric("output-count", number(data.total_outputs));
    updateMetric("block-count", number(data.total_blocks));
    $("uptime").textContent = uptime(data.uptime_seconds);
    $("network").textContent = data.network;
    $("version").textContent = `Whisper ${data.version} · Python`;
    $("footer-version").textContent = `v${data.version}`;
    $("node-tip").textContent =
      data.node_height === null
        ? "Node tip unavailable"
        : `Node tip #${number(data.node_height)}`;
    $("coverage").textContent = data.total_blocks
      ? `#${number(data.start_height)} to #${number(data.tip_height)}`
      : "Waiting for the first block";
    state("api-state", `${elapsed} ms`, "good");
    state("db-state", "Connected", "good");
    const disabled = data.indexer === "disabled";
    const failed = ["error", "stopped"].includes(data.indexer);
    state(
      "rpc-state",
      disabled
        ? "Not checked"
        : failed
          ? "Needs attention"
          : data.node_height === null
            ? "Connecting"
            : "Connected",
      disabled ? "" : failed ? "warn" : data.node_height === null ? "" : "good",
    );
    const statuses = {
      starting: "Starting",
      syncing: "Catching up",
      synced: "Up to date",
      disabled: "Disabled",
      error: "Retrying",
      stopped: "Stopped",
    };
    state(
      "indexer-state",
      statuses[data.indexer] || "Unknown",
      data.indexer === "synced" ? "good" : "warn",
    );
    state(
      "overall",
      disabled
        ? "Indexer disabled"
        : data.status === "ok"
          ? "Operational"
          : "Needs attention",
      disabled || data.status !== "ok" ? "warn" : "good",
    );
    $("connection-note").textContent = disabled
      ? "Enable INDEXER_ENABLED to follow Bitcoin Core."
      : data.reorgs
        ? `${data.reorgs} chain reorganization(s) handled this session.`
        : "RPC catches up on restart. Keys stay in your client.";
    if (disabled)
      notice(
        "Indexer is disabled",
        "The API is available. Enable INDEXER_ENABLED in .env and restart to index blocks.",
      );
    else if (data.status !== "ok")
      notice(
        "Indexer needs attention",
        data.indexer_error ||
          "Connecting to Bitcoin Core. Existing indexed data remains available.",
      );
    else if (!data.total_blocks)
      notice(
        "No blocks indexed yet",
        "Check START_HEIGHT and your node height. New blocks will appear here as indexing progresses.",
      );
    else notice(null);
    lastUpdate = Date.now();
    document.querySelector(".metrics").classList.remove("stale");
    updateFreshness();
  }

  function emptyBlocks(title, message) {
    const row = document.createElement("tr"),
      cell = document.createElement("td");
    cell.colSpan = 4;
    cell.className = "empty-cell";
    const strong = document.createElement("strong"),
      span = document.createElement("span");
    strong.textContent = title;
    span.textContent = message;
    cell.append(strong, span);
    row.append(cell);
    $("blocks-body").replaceChildren(row);
  }

  function renderBlocks(result) {
    if (!result.response.ok || !Array.isArray(result.data?.blocks))
      throw new Error("Blocks unavailable");
    const blocks = result.data.blocks;
    $("blocks-note").textContent = "";
    if (!blocks.length)
      return emptyBlocks(
        "Your index starts here.",
        "Connect Bitcoin Core to see blocks and Taproot outputs as they are indexed.",
      );
    const rows = blocks.map((block) => {
      const row = document.createElement("tr");
      const height = document.createElement("td");
      height.textContent = `#${number(block.height)}`;
      const hash = document.createElement("td"),
        copy = document.createElement("button");
      copy.type = "button";
      copy.className = "hash-button";
      copy.textContent = `${block.hash.slice(0, 8)}…${block.hash.slice(-6)}`;
      copy.title = `Copy block hash ${block.hash}`;
      copy.setAttribute("aria-label", `Copy hash for block ${block.height}`);
      copy.addEventListener("click", () =>
        copyText(block.hash, "Block hash copied"),
      );
      hash.append(copy);
      const outputs = document.createElement("td");
      outputs.className = "numeric";
      outputs.textContent = number(block.output_count);
      const time = document.createElement("td");
      time.className = "numeric";
      const date = new Date(block.timestamp * 1000);
      time.textContent = new Intl.DateTimeFormat(undefined, {
        hour: "2-digit",
        minute: "2-digit",
      }).format(date);
      time.title = date.toLocaleString();
      row.append(height, hash, outputs, time);
      return row;
    });
    $("blocks-body").replaceChildren(...rows);
  }

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    $("refresh").disabled = true;
    $("refresh").textContent = "Refreshing…";
    clearTimeout(timer);
    const [statusResult, blocksResult] = await Promise.allSettled([
      fetchJSON("/api/v1/status"),
      fetchJSON("/api/v1/blocks?limit=6"),
    ]);
    try {
      if (statusResult.status === "rejected") throw statusResult.reason;
      renderStatus(statusResult.value);
    } catch (error) {
      state("overall", "Unavailable", "bad");
      state(
        "api-state",
        statusResult.status === "fulfilled" ? "Service error" : "Unreachable",
        "bad",
      );
      state("db-state", "Unconfirmed");
      state("rpc-state", "Unconfirmed");
      state("indexer-state", "Unconfirmed");
      document.querySelector(".metrics").classList.add("stale");
      notice(
        "Live status is unavailable",
        lastUpdate
          ? "Showing the last known values. Check the server connection and refresh to retry."
          : "Start the Whisper server and check its database connection. Refresh to retry.",
      );
      updateFreshness();
    }
    try {
      if (blocksResult.status === "rejected") throw blocksResult.reason;
      renderBlocks(blocksResult.value);
    } catch {
      if (!$("blocks-body").querySelector(".hash-button"))
        emptyBlocks(
          "Blocks are unavailable.",
          "Check the server connection, then select Refresh.",
        );
      $("blocks-note").textContent = "Could not refresh";
    }
    refreshing = false;
    $("refresh").disabled = false;
    $("refresh").textContent = "Refresh";
    if (!paused && !document.hidden) timer = setTimeout(refresh, 5000);
  }

  function updateFreshness() {
    if (!lastUpdate) {
      $("freshness").textContent = "No successful update yet";
      return;
    }
    const seconds = Math.max(0, Math.floor((Date.now() - lastUpdate) / 1000));
    const age =
      seconds < 5
        ? "just now"
        : seconds < 60
          ? `${seconds}s ago`
          : `${Math.floor(seconds / 60)}m ago`;
    $("freshness").textContent = `${paused ? "Paused · " : ""}Updated ${age}`;
  }

  function buildRequest(validate = true) {
    const key = $("endpoint").value,
      endpoint = endpoints[key];
    let path = endpoint.path,
      body;
    if (key === "scan" || key === "transactions") {
      const start = Number($("start-height").value),
        end = Number($("end-height").value);
      if (
        validate &&
        (!$("start-height").value ||
          !$("end-height").value ||
          !Number.isSafeInteger(start) ||
          !Number.isSafeInteger(end) ||
          start < 0 ||
          end < start ||
          end > 2147483647)
      ) {
        throw new Error(
          "Enter whole block heights, with the end at or after the start.",
        );
      }
      if (validate && end - start + 1 > limits.max_block_range)
        throw new Error(
          `Request at most ${number(limits.max_block_range)} blocks at a time (inclusive).`,
        );
      if (key === "scan") {
        const prefixes = [
          ...new Set(
            $("prefixes")
              .value.trim()
              .split(/[\s,]+/)
              .filter(Boolean)
              .map((p) => p.toLowerCase()),
          ),
        ];
        if (
          validate &&
          (!prefixes.length || prefixes.some((p) => !/^[0-9a-f]{8}$/.test(p)))
        )
          throw new Error(
            "Enter at least one prefix. Each prefix needs exactly eight hexadecimal characters.",
          );
        if (validate && prefixes.length > limits.max_prefixes)
          throw new Error(
            `Use at most ${number(limits.max_prefixes)} prefixes.`,
          );
        body = { start_height: start, end_height: end, prefixes };
      } else path += `?start_height=${start}&end_height=${end}`;
    }
    return { key, method: endpoint.method, path, body };
  }

  function updateRequest() {
    const key = $("endpoint").value,
      endpoint = endpoints[key];
    $("method").textContent = endpoint.method;
    $("endpoint-help").textContent = endpoint.help;
    $("range-fields").hidden = !["scan", "transactions"].includes(key);
    $("prefix-field").hidden = key !== "scan";
    const request = buildRequest(false);
    $("request-path").textContent = `${request.method} ${request.path}`;
    $("form-error").hidden = true;
    nextCursor = null;
    $("next-page").hidden = true;
  }

  function showFormError(message) {
    $("form-error").textContent = message;
    $("form-error").hidden = false;
  }

  async function sendRequest(nextPage = false) {
    if (sending) return;
    let request;
    try {
      request = nextPage
        ? {
            ...lastRequest,
            body: lastRequest.body ? { ...lastRequest.body } : undefined,
          }
        : buildRequest();
    } catch (error) {
      showFormError(error.message);
      return;
    }
    if (nextPage && nextCursor) {
      if (request.body) request.body.cursor = nextCursor;
      else
        request.path += `${request.path.includes("?") ? "&" : "?"}cursor=${encodeURIComponent(nextCursor)}`;
    }
    if (!nextPage) lastRequest = request;
    sending = true;
    $("send").disabled = true;
    $("next-page").disabled = true;
    $("send").textContent = "Sending…";
    $("response-panel").setAttribute("aria-busy", "true");
    $("form-error").hidden = true;
    try {
      const options = { method: request.method };
      if (request.body) {
        options.headers = { "Content-Type": "application/json" };
        options.body = JSON.stringify(request.body);
      }
      const result = await fetchJSON(request.path, options);
      responseText =
        result.data !== null
          ? JSON.stringify(result.data, null, 2)
          : result.text;
      $("response-empty").hidden = true;
      $("response-body").hidden = false;
      $("response-body").textContent =
        responseText.length > 200000
          ? `${responseText.slice(0, 200000)}\n\n[Display truncated. Copy JSON for the full response.]`
          : responseText;
      state(
        "response-status",
        `HTTP ${result.response.status}${result.response.ok ? " · Success" : " · Request failed"}`,
        result.response.ok ? "good" : "bad",
      );
      $("response-time").textContent = `${result.elapsed} ms`;
      $("copy-response").disabled = !responseText;
      nextCursor = result.response.ok ? result.data?.next_cursor || null : null;
      $("next-page").hidden = !nextCursor;
      $("response-footer").hidden = false;
      const count =
        result.data?.candidates?.length ?? result.data?.transactions?.length;
      $("page-info").textContent =
        count !== undefined
          ? `${number(count)} ${request.key === "scan" ? "candidate(s)" : "transaction(s)"}${nextCursor ? " · More available" : " · End of results"}`
          : result.response.ok
            ? "Response received"
            : "Check the response for details";
    } catch (error) {
      state("response-status", "Request unavailable", "bad");
      $("response-time").textContent = "";
      responseText =
        error.name === "AbortError"
          ? "The request took more than 15 seconds. Check the connection and try again."
          : "Could not reach the server. Check the connection and try again.";
      $("response-empty").hidden = true;
      $("response-body").hidden = false;
      $("response-body").textContent = responseText;
      $("copy-response").disabled = true;
      $("response-footer").hidden = true;
      nextCursor = null;
    } finally {
      sending = false;
      $("send").disabled = false;
      $("send").textContent = "Send request ↗";
      $("next-page").disabled = false;
      $("response-panel").setAttribute("aria-busy", "false");
    }
  }

  function toast(message) {
    $("toast").textContent = message;
    $("toast").hidden = false;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      $("toast").hidden = true;
    }, 3000);
  }
  async function copyText(text, message) {
    try {
      await navigator.clipboard.writeText(text);
      toast(message);
    } catch {
      toast("Clipboard unavailable. Select the text and copy it manually.");
    }
  }
  const quote = (text) => `'${text.replaceAll("'", "'\\''")}'`;

  $("refresh").addEventListener("click", refresh);
  $("pause").addEventListener("click", () => {
    paused = !paused;
    $("pause").textContent = paused ? "Resume updates" : "Pause updates";
    $("pause").setAttribute("aria-pressed", String(paused));
    clearTimeout(timer);
    updateFreshness();
    if (!paused) refresh();
  });
  document.addEventListener("visibilitychange", () => {
    clearTimeout(timer);
    if (!document.hidden && !paused) refresh();
  });
  $("endpoint").addEventListener("change", updateRequest);
  for (const id of ["start-height", "end-height", "prefixes"])
    $(id).addEventListener("input", updateRequest);
  $("request-form").addEventListener("submit", (event) => {
    event.preventDefault();
    sendRequest();
  });
  $("request-form").addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      sendRequest();
    }
  });
  $("next-page").addEventListener("click", () => sendRequest(true));
  $("copy-response").addEventListener("click", () =>
    copyText(responseText, "Response copied"),
  );
  $("copy-request").addEventListener("click", () => {
    try {
      const request = buildRequest();
      let command = `curl ${quote(location.origin + request.path)}`;
      if (request.body)
        command += ` -H 'Content-Type: application/json' --data ${quote(JSON.stringify(request.body))}`;
      copyText(command, "cURL command copied");
    } catch (error) {
      showFormError(error.message);
    }
  });
  updateRequest();
  refresh();
  setInterval(updateFreshness, 1000);
})();
