/**
 * WHISPER Dashboard — Client-Side Logic
 * Auto-refreshing status, API explorer, live updates
 */

(function () {
    'use strict';

    // ── Config ──────────────────────────────────────────
    const REFRESH_INTERVAL = 5000; // 5 seconds
    const API_BASE = window.location.origin;

    // ── DOM References ──────────────────────────────────
    const $ = (sel) => document.querySelector(sel);
    const els = {
        tipHeight:    $('#tip-height'),
        totalOutputs: $('#total-outputs'),
        totalBlocks:  $('#total-blocks'),
        uptime:       $('#uptime'),
        networkBadge: $('#network-badge'),
        versionBadge: $('#version-badge'),
        statusBadge:  $('#status-badge'),
        dbDot:        $('#db-dot'),
        dbStatus:     $('#db-status'),
        indexerDot:   $('#indexer-dot'),
        indexerStatus:$('#indexer-status'),
        apiDot:       $('#api-dot'),
        apiStatus:    $('#api-status'),
        networkDot:   $('#network-dot'),
        networkValue: $('#network-value'),
        lastRefresh:  $('#last-refresh'),
        apiMethod:    $('#api-method'),
        apiEndpoint:  $('#api-endpoint'),
        apiBody:      $('#api-body'),
        bodyRow:      $('#body-row'),
        sendBtn:      $('#send-btn'),
        responseBox:  $('#response-box'),
        responseStatus: $('#response-status'),
        responseTime: $('#response-time'),
        responseBody: $('#response-body'),
    };

    // ── State ───────────────────────────────────────────
    let lastRefreshTime = Date.now();
    let isOnline = false;

    // ── Formatting Helpers ──────────────────────────────

    function formatNumber(n) {
        if (n === null || n === undefined || n === '—') return '—';
        return Number(n).toLocaleString('en-US');
    }

    function formatUptime(seconds) {
        if (!seconds && seconds !== 0) return '—';
        const s = Math.floor(seconds);
        const days = Math.floor(s / 86400);
        const hours = Math.floor((s % 86400) / 3600);
        const mins = Math.floor((s % 3600) / 60);
        const secs = s % 60;

        if (days > 0) return `${days}d ${hours}h ${mins}m`;
        if (hours > 0) return `${hours}h ${mins}m ${secs}s`;
        if (mins > 0) return `${mins}m ${secs}s`;
        return `${secs}s`;
    }

    function timeAgo(ms) {
        const seconds = Math.floor((Date.now() - ms) / 1000);
        if (seconds < 5) return 'just now';
        if (seconds < 60) return `${seconds}s ago`;
        const mins = Math.floor(seconds / 60);
        return `${mins}m ago`;
    }

    function setDot(dotEl, status) {
        dotEl.className = 'status-dot status-dot--' + status;
    }

    // ── Status Fetching ─────────────────────────────────

    async function fetchStatus() {
        try {
            const start = performance.now();
            const res = await fetch(`${API_BASE}/api/v1/status`, {
                signal: AbortSignal.timeout(8000),
            });
            const elapsed = Math.round(performance.now() - start);

            if (!res.ok) throw new Error(`HTTP ${res.status}`);

            const data = await res.json();
            updateDashboard(data, elapsed);
            setOnline(true);
        } catch (err) {
            console.warn('Status fetch failed:', err.message);
            setOnline(false);
        }
    }

    function updateDashboard(data, latencyMs) {
        // Stat cards
        animateValue(els.tipHeight, formatNumber(data.tip_height));
        animateValue(els.totalOutputs, formatNumber(data.total_outputs));
        animateValue(els.totalBlocks, formatNumber(data.total_blocks));
        animateValue(els.uptime, formatUptime(data.uptime_seconds));

        // Badges
        els.networkBadge.textContent = data.network || 'unknown';
        if (data.version) {
            els.versionBadge.textContent = 'v' + data.version;
        }

        // Status rows
        setDot(els.dbDot, 'success');
        els.dbStatus.textContent = 'Connected';

        const synced = data.tip_height > 0;
        setDot(els.indexerDot, synced ? 'success' : 'warning');
        els.indexerStatus.textContent = synced
            ? `Synced to #${formatNumber(data.tip_height)}`
            : 'Waiting for blocks';

        setDot(els.apiDot, 'success');
        els.apiStatus.textContent = `${latencyMs}ms latency`;

        setDot(els.networkDot, 'info');
        els.networkValue.textContent = data.network || '—';

        // Refresh timestamp
        lastRefreshTime = Date.now();
        els.lastRefresh.textContent = 'Updated just now';
    }

    function setOnline(online) {
        isOnline = online;
        const badge = els.statusBadge;
        badge.className = 'badge badge--status ' + (online ? 'online' : 'offline');
        badge.innerHTML = `<span class="pulse"></span> ${online ? 'online' : 'offline'}`;

        if (!online) {
            setDot(els.dbDot, 'danger');
            els.dbStatus.textContent = 'Unreachable';
            setDot(els.indexerDot, 'danger');
            els.indexerStatus.textContent = 'Unreachable';
            setDot(els.apiDot, 'danger');
            els.apiStatus.textContent = 'Unreachable';
        }
    }

    // ── Value Animation ─────────────────────────────────

    function animateValue(el, newValue) {
        if (el.textContent === newValue) return;
        el.style.opacity = '0.4';
        el.style.transform = 'translateY(4px)';
        setTimeout(() => {
            el.textContent = newValue;
            el.style.opacity = '1';
            el.style.transform = 'translateY(0)';
        }, 150);
    }

    // ── API Explorer ────────────────────────────────────

    function setupExplorer() {
        // Sync method and endpoint selects
        els.apiEndpoint.addEventListener('change', () => {
            const opt = els.apiEndpoint.selectedOptions[0];
            const method = opt.dataset.method || 'GET';
            els.apiMethod.value = method;
            els.bodyRow.style.display = method === 'POST' ? 'flex' : 'none';
        });

        els.apiMethod.addEventListener('change', () => {
            els.bodyRow.style.display = els.apiMethod.value === 'POST' ? 'flex' : 'none';
        });

        els.sendBtn.addEventListener('click', sendRequest);
    }

    async function sendRequest() {
        const method = els.apiMethod.value;
        const endpoint = els.apiEndpoint.value;
        const url = `${API_BASE}${endpoint}`;

        els.sendBtn.disabled = true;
        els.sendBtn.innerHTML = `
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" class="spin">
                <circle cx="7" cy="7" r="5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-dasharray="20" stroke-dashoffset="10"/>
            </svg>
            Sending...
        `;

        const startTime = performance.now();

        try {
            const options = { method };

            if (method === 'POST') {
                options.headers = { 'Content-Type': 'application/json' };
                options.body = els.apiBody.value;
                // Validate JSON
                try { JSON.parse(options.body); } catch {
                    showResponse(400, 'Invalid JSON in request body', 0);
                    return;
                }
            }

            const res = await fetch(url, options);
            const elapsed = Math.round(performance.now() - startTime);
            const text = await res.text();

            let formatted;
            try {
                formatted = JSON.stringify(JSON.parse(text), null, 2);
            } catch {
                formatted = text;
            }

            showResponse(res.status, formatted, elapsed);
        } catch (err) {
            const elapsed = Math.round(performance.now() - startTime);
            showResponse(0, `Request failed: ${err.message}`, elapsed);
        } finally {
            els.sendBtn.disabled = false;
            els.sendBtn.innerHTML = `
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <path d="M1 7h12M8 2l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                Send Request
            `;
        }
    }

    function showResponse(status, body, elapsed) {
        els.responseBox.style.display = 'block';

        const isOk = status >= 200 && status < 300;
        els.responseStatus.textContent = status > 0 ? `${status} ${isOk ? 'OK' : 'Error'}` : 'Network Error';
        els.responseStatus.className = 'response-box__status ' + (isOk ? 'success' : 'error');
        els.responseTime.textContent = `${elapsed}ms`;
        els.responseBody.textContent = body;

        // Scroll response into view
        els.responseBox.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    }

    // ── Refresh Timer Label ─────────────────────────────

    function updateRefreshLabel() {
        if (els.lastRefresh) {
            els.lastRefresh.textContent = 'Updated ' + timeAgo(lastRefreshTime);
        }
    }

    // ── CSS for spinner ─────────────────────────────────

    function injectSpinnerCSS() {
        const style = document.createElement('style');
        style.textContent = `
            @keyframes spin { to { transform: rotate(360deg); } }
            .spin { animation: spin 0.8s linear infinite; }
            .stat-card__value, .status-row__value {
                transition: opacity 0.15s ease, transform 0.15s ease;
            }
        `;
        document.head.appendChild(style);
    }

    // ── Init ────────────────────────────────────────────

    function init() {
        injectSpinnerCSS();
        setupExplorer();
        fetchStatus();

        // Auto-refresh status
        setInterval(fetchStatus, REFRESH_INTERVAL);
        // Update "Updated X ago" label
        setInterval(updateRefreshLabel, 1000);
    }

    // Start when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
