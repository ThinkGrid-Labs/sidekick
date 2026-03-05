/**
 * Sidekick React Native SDK
 *
 * Bridges the JS layer to the Rust evaluation engine via the C++ JSI host
 * installed as `global.__SidekickInternal` by `installSidekickJSI()`.
 *
 * The server now sends the full flag state on every SSE (re)connect, so:
 *   1. Open SSE stream.
 *   2. On "connected" → clear the Rust cache.
 *   3. Incoming UPSERT events rebuild the cache.
 *   4. isEnabled() evaluates synchronously — zero network IO, sub-microsecond.
 */
export class SidekickMobileClient {
    constructor(serverUrl, sdkKey) {
        this.serverUrl = serverUrl;
        this.sdkKey = sdkKey;
        this.initialized = false;
        this.sse = null;
        // The internal module exposed by the C++ JSI installation
        this.bridge = global.__SidekickInternal;
    }

    async init() {
        if (this.initialized) return;

        if (!this.bridge) {
            throw new Error(
                '[Sidekick] JSI module not found. Ensure installSidekickJSI() was called ' +
                'in your native module before JS starts.'
            );
        }

        this._connectDeltas();
        this.initialized = true;
    }

    _connectDeltas() {
        if (this.sse) {
            this.sse.close();
        }

        // React Native's built-in fetch-based EventSource (or the `event-source`
        // package) supports headers, so we send auth via Authorization header.
        this.sse = new EventSource(`${this.serverUrl}/stream`, {
            headers: { 'Authorization': `Bearer ${this.sdkKey}` }
        });

        // Server sends "connected" before the full-state dump on every (re)connect.
        // Clear the Rust cache so stale / deleted flags are evicted first.
        this.sse.addEventListener('connected', () => {
            this.bridge.clearStore();
            console.log('[Sidekick] Stream connected — cache cleared, rebuilding from server state.');
        });

        this.sse.addEventListener('update', (e) => {
            try {
                const event = JSON.parse(e.data);

                if (event.type === 'UPSERT') {
                    const f = event.flag;
                    // Pass rules as a JS array — the JSI layer JSON.stringifies it before
                    // crossing into Rust, so all targeting rules are preserved.
                    this.bridge.upsertFlag(
                        f.key,
                        f.is_enabled,
                        f.rollout_percentage ?? -1,
                        f.rules || []
                    );
                } else if (event.type === 'DELETE') {
                    this.bridge.deleteFlag(event.key);
                }
            } catch (err) {
                console.error('[Sidekick] Failed to parse delta update:', err);
            }
        });

        this.sse.onerror = () => {
            console.warn('[Sidekick] Stream disconnected — EventSource will reconnect automatically.');
        };
    }

    /**
     * Evaluates a feature flag synchronously.
     * The call crosses JS → C++ JSI → Rust and returns in sub-microsecond time.
     *
     * @param {string} flagKey
     * @param {string} userKey        Stable user identifier used for rollout hashing.
     * @param {Object} [attributes]   Flat key→value map of user attributes for targeting rules.
     */
    isEnabled(flagKey, userKey, attributes = {}) {
        if (!this.initialized || !this.bridge) return false;
        return this.bridge.isEnabled(flagKey, userKey, attributes);
    }

    close() {
        if (this.sse) {
            this.sse.close();
            this.sse = null;
        }
    }
}
