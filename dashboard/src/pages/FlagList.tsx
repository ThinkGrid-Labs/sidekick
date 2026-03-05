import { useEffect, useState, useCallback } from 'react'
import { Link } from 'react-router-dom'
import { api } from '../api'
import type { Flag } from '../types'

export default function FlagList() {
  const [flags, setFlags] = useState<Flag[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      setError(null)
      const data = await api.listFlags()
      setFlags(data)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load flags')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { void load() }, [load])

  async function toggleEnabled(flag: Flag) {
    try {
      const updated = await api.patchFlag(flag.key, { is_enabled: !flag.is_enabled })
      setFlags(prev => prev.map(f => (f.key === flag.key ? updated : f)))
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Update failed')
    }
  }

  async function handleDelete(key: string) {
    if (!confirm(`Delete flag "${key}"?`)) return
    try {
      await api.deleteFlag(key)
      setFlags(prev => prev.filter(f => f.key !== key))
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Delete failed')
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-40 text-gray-500">
        Loading flags...
      </div>
    )
  }

  if (error) {
    return (
      <div className="rounded-md bg-red-50 p-4 text-red-700">
        <p className="font-medium">Error</p>
        <p className="text-sm mt-1">{error}</p>
        <button
          onClick={() => { setLoading(true); void load() }}
          className="mt-3 text-sm font-medium text-red-600 underline"
        >
          Retry
        </button>
      </div>
    )
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-semibold text-gray-900">Feature Flags</h1>
        <Link
          to="/flags/new"
          className="inline-flex items-center px-4 py-2 bg-indigo-600 text-white text-sm font-medium rounded-md hover:bg-indigo-700 transition-colors"
        >
          + New Flag
        </Link>
      </div>

      {flags.length === 0 ? (
        <div className="text-center py-16 text-gray-400">
          <p className="text-lg">No flags yet.</p>
          <Link to="/flags/new" className="mt-2 text-indigo-600 hover:underline text-sm">
            Create your first flag
          </Link>
        </div>
      ) : (
        <div className="bg-white shadow rounded-lg overflow-hidden">
          <table className="min-w-full divide-y divide-gray-200">
            <thead className="bg-gray-50">
              <tr>
                <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Key</th>
                <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Description</th>
                <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Rollout</th>
                <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Rules</th>
                <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Enabled</th>
                <th className="px-6 py-3" />
              </tr>
            </thead>
            <tbody className="bg-white divide-y divide-gray-200">
              {flags.map(flag => (
                <tr key={flag.key} className="hover:bg-gray-50">
                  <td className="px-6 py-4">
                    <span className="font-mono text-sm text-gray-900">{flag.key}</span>
                  </td>
                  <td className="px-6 py-4 text-sm text-gray-500 max-w-xs truncate">
                    {flag.description ?? <span className="text-gray-300">—</span>}
                  </td>
                  <td className="px-6 py-4 text-sm text-gray-700">
                    {flag.rollout_percentage != null
                      ? `${flag.rollout_percentage}%`
                      : <span className="text-gray-400">100%</span>}
                  </td>
                  <td className="px-6 py-4 text-sm text-gray-700">
                    {flag.rules.length > 0
                      ? <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-indigo-100 text-indigo-800">{flag.rules.length} rule{flag.rules.length !== 1 ? 's' : ''}</span>
                      : <span className="text-gray-400">—</span>}
                  </td>
                  <td className="px-6 py-4">
                    <button
                      onClick={() => void toggleEnabled(flag)}
                      className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none ${
                        flag.is_enabled ? 'bg-indigo-600' : 'bg-gray-200'
                      }`}
                      aria-label={flag.is_enabled ? 'Disable flag' : 'Enable flag'}
                    >
                      <span
                        className={`inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${
                          flag.is_enabled ? 'translate-x-6' : 'translate-x-1'
                        }`}
                      />
                    </button>
                  </td>
                  <td className="px-6 py-4 text-right text-sm font-medium space-x-3">
                    <Link
                      to={`/flags/${encodeURIComponent(flag.key)}/edit`}
                      className="text-indigo-600 hover:text-indigo-900"
                    >
                      Edit
                    </Link>
                    <button
                      onClick={() => void handleDelete(flag.key)}
                      className="text-red-500 hover:text-red-700"
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
