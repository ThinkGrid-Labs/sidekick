const { SidekickCore } = require('./nodejs.node');
const EventSource = require('eventsource');

class SidekickClient {
    constructor(serverUrl, sdkKey) {
        this.serverUrl = serverUrl;
        this.sdkKey = sdkKey;
        this.core = new SidekickCore();
        this.initialized = false;
        this.sse = null;
    }

    /**
     * Opens the SSE connection. The server now sends:
     *   1. "connected" event  → clear local cache
     *   2. "update" UPSERT events for every live flag (full bootstrap)
     *   3. Ongoing "update" UPSERT/DELETE deltas
     *
     * On reconnect EventSource calls this automatically, so the cache is
     * rebuilt cleanly and any flags deleted while disconnected are evicted.
     */
    async init() {
        if (this.initialized) return;
        this._connectDeltas();
        this.initialized = true;
    }

    _connectDeltas() {
        if (this.sse) {
            this.sse.close();
        }

        this.sse = new EventSource(`${this.serverUrl}/stream`, {
            headers: { 'Authorization': `Bearer ${this.sdkKey}` }
        });

        // Server sends "connected" before the full-state dump on every (re)connect.
        // Clear the cache so stale/deleted flags from the previous session are evicted.
        this.sse.addEventListener('connected', () => {
            this.core.clearStore();
            console.log('[Sidekick] Stream connected — cache cleared, rebuilding from server state.');
        });

        this.sse.addEventListener('update', (e) => {
            try {
                const event = JSON.parse(e.data);

                if (event.type === 'UPSERT') {
                    const f = event.flag;
                    this.core.upsertFlag(
                        f.key,
                        f.is_enabled,
                        f.rollout_percentage ?? null,
                        f.description ?? null,
                        f.rules || []
                    );
                } else if (event.type === 'DELETE') {
                    // Properly remove the flag from cache instead of zombie-upserting it.
                    this.core.deleteFlag(event.key);
                }
            } catch (err) {
                console.error('[Sidekick] Failed to parse delta update:', err);
            }
        });

        this.sse.onerror = () => {
            // EventSource reconnects automatically; no action needed here.
            console.warn('[Sidekick] Stream disconnected — EventSource will reconnect automatically.');
        };
    }

    /**
     * Evaluates a feature flag in <1 microsecond with 0 network IO.
     */
    isEnabled(flagKey, userKey, userAttributes = {}) {
        if (!this.initialized) {
            console.warn('[Sidekick] Evaluated flag before init! Returning false.');
            return false;
        }
        return this.core.isEnabled(flagKey, userKey, userAttributes);
    }

    close() {
        if (this.sse) {
            this.sse.close();
            this.sse = null;
        }
    }
}

module.exports = { SidekickClient };
