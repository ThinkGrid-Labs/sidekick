import { useState, useEffect } from 'react'

export default function Settings() {
  const [sdkKey, setSdkKey] = useState('')
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    setSdkKey(localStorage.getItem('sidekick_sdk_key') ?? '')
  }, [])

  function handleSave(e: React.FormEvent) {
    e.preventDefault()
    if (sdkKey.trim()) {
      localStorage.setItem('sidekick_sdk_key', sdkKey.trim())
    } else {
      localStorage.removeItem('sidekick_sdk_key')
    }
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  function handleClear() {
    setSdkKey('')
    localStorage.removeItem('sidekick_sdk_key')
    setSaved(true)
    setTimeout(() => setSaved(false), 2000)
  }

  return (
    <div className="max-w-lg">
      <h1 className="text-2xl font-semibold text-gray-900 mb-6">Settings</h1>

      <div className="bg-white shadow rounded-lg p-6 space-y-6">
        <div>
          <h2 className="text-base font-medium text-gray-900 mb-1">Authentication</h2>
          <p className="text-sm text-gray-500 mb-4">
            Your SDK key is sent as{' '}
            <code className="bg-gray-100 px-1 rounded text-xs font-mono">Authorization: Bearer &lt;key&gt;</code>{' '}
            on every API request. It is stored in <code className="bg-gray-100 px-1 rounded text-xs font-mono">localStorage</code> and never sent elsewhere.
          </p>

          <form onSubmit={e => void handleSave(e)} className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">SDK Key</label>
              <input
                type="password"
                value={sdkKey}
                onChange={e => { setSdkKey(e.target.value); setSaved(false) }}
                placeholder="sk-..."
                className="w-full border border-gray-300 rounded-md px-3 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
            </div>

            <div className="flex items-center gap-3">
              <button
                type="submit"
                className="px-4 py-2 bg-indigo-600 text-white text-sm font-medium rounded-md hover:bg-indigo-700 transition-colors"
              >
                Save
              </button>
              <button
                type="button"
                onClick={handleClear}
                className="px-4 py-2 text-sm font-medium text-gray-600 hover:text-gray-900"
              >
                Clear
              </button>
              {saved && (
                <span className="text-sm text-green-600 font-medium">Saved.</span>
              )}
            </div>
          </form>
        </div>

        <hr />

        <div>
          <h2 className="text-base font-medium text-gray-900 mb-1">API</h2>
          <p className="text-sm text-gray-500">
            The dashboard talks directly to the Sidekick server at{' '}
            <code className="bg-gray-100 px-1 rounded text-xs font-mono">
              {import.meta.env.VITE_API_URL || window.location.origin}
            </code>
            . Set <code className="bg-gray-100 px-1 rounded text-xs font-mono">VITE_API_URL</code> at
            build time to point at a different server.
          </p>
        </div>
      </div>
    </div>
  )
}
