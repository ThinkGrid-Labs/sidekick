import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { api } from '../api'
import type { Flag, TargetingRule } from '../types'
import RuleEditor from '../components/RuleEditor'

const EMPTY_FLAG: Flag = {
  key: '',
  is_enabled: true,
  rollout_percentage: null,
  description: null,
  rules: [],
}

export default function FlagEditor() {
  const { key } = useParams<{ key?: string }>()
  const isEdit = Boolean(key)
  const navigate = useNavigate()

  const [flag, setFlag] = useState<Flag>(EMPTY_FLAG)
  const [rolloutInput, setRolloutInput] = useState('')
  const [loading, setLoading] = useState(isEdit)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!key) return
    api.getFlag(key)
      .then(f => {
        setFlag(f)
        setRolloutInput(f.rollout_percentage != null ? String(f.rollout_percentage) : '')
      })
      .catch(e => setError(e instanceof Error ? e.message : 'Failed to load flag'))
      .finally(() => setLoading(false))
  }, [key])

  function setField<K extends keyof Flag>(field: K, value: Flag[K]) {
    setFlag(prev => ({ ...prev, [field]: value }))
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setSaving(true)
    setError(null)

    const rollout = rolloutInput.trim() === '' ? null : parseInt(rolloutInput, 10)
    if (rollout !== null && (isNaN(rollout) || rollout < 0 || rollout > 100)) {
      setError('Rollout percentage must be 0–100 or empty (for 100%).')
      setSaving(false)
      return
    }

    const payload: Flag = { ...flag, rollout_percentage: rollout }

    try {
      if (isEdit && key) {
        await api.patchFlag(key, {
          is_enabled: payload.is_enabled,
          rollout_percentage: payload.rollout_percentage,
          description: payload.description,
          rules: payload.rules,
        })
      } else {
        await api.createFlag(payload)
      }
      navigate('/')
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Save failed')
    } finally {
      setSaving(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-40 text-gray-500">
        Loading flag...
      </div>
    )
  }

  return (
    <div className="max-w-2xl">
      <div className="flex items-center gap-3 mb-6">
        <button
          onClick={() => navigate('/')}
          className="text-gray-400 hover:text-gray-600 text-sm"
        >
          ← Flags
        </button>
        <h1 className="text-2xl font-semibold text-gray-900">
          {isEdit ? `Edit: ${key}` : 'New Flag'}
        </h1>
      </div>

      {error && (
        <div className="mb-4 rounded-md bg-red-50 p-3 text-sm text-red-700">
          {error}
        </div>
      )}

      <form onSubmit={e => void handleSubmit(e)} className="bg-white shadow rounded-lg p-6 space-y-6">
        {/* Key */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Key <span className="text-red-500">*</span>
          </label>
          <input
            type="text"
            required
            disabled={isEdit}
            value={flag.key}
            onChange={e => setField('key', e.target.value)}
            placeholder="e.g. dark_mode"
            className="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 disabled:bg-gray-100 disabled:text-gray-500 font-mono"
          />
          {!isEdit && (
            <p className="mt-1 text-xs text-gray-400">Immutable after creation. Use snake_case.</p>
          )}
        </div>

        {/* Description */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">Description</label>
          <input
            type="text"
            value={flag.description ?? ''}
            onChange={e => setField('description', e.target.value || null)}
            placeholder="What does this flag do?"
            className="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
          />
        </div>

        {/* Enabled */}
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => setField('is_enabled', !flag.is_enabled)}
            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none ${
              flag.is_enabled ? 'bg-indigo-600' : 'bg-gray-200'
            }`}
            aria-label="Toggle enabled"
          >
            <span
              className={`inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform ${
                flag.is_enabled ? 'translate-x-6' : 'translate-x-1'
              }`}
            />
          </button>
          <span className="text-sm font-medium text-gray-700">
            {flag.is_enabled ? 'Enabled' : 'Disabled'}
          </span>
        </div>

        {/* Rollout */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-1">
            Rollout percentage
          </label>
          <div className="flex items-center gap-2">
            <input
              type="number"
              min={0}
              max={100}
              value={rolloutInput}
              onChange={e => setRolloutInput(e.target.value)}
              placeholder="100 (default)"
              className="w-32 border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
            <span className="text-sm text-gray-500">%</span>
          </div>
          <p className="mt-1 text-xs text-gray-400">
            Leave empty for 100%. Users are bucketed deterministically by their key.
          </p>
        </div>

        {/* Targeting rules */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">
            Targeting rules
          </label>
          <p className="text-xs text-gray-400 mb-3">
            Users matching any rule bypass the rollout cap and always see the flag as enabled.
          </p>
          <RuleEditor
            rules={flag.rules as TargetingRule[]}
            onChange={rules => setField('rules', rules)}
          />
        </div>

        {/* Actions */}
        <div className="flex items-center gap-3 pt-2 border-t border-gray-100">
          <button
            type="submit"
            disabled={saving}
            className="px-4 py-2 bg-indigo-600 text-white text-sm font-medium rounded-md hover:bg-indigo-700 disabled:opacity-60 transition-colors"
          >
            {saving ? 'Saving...' : isEdit ? 'Save changes' : 'Create flag'}
          </button>
          <button
            type="button"
            onClick={() => navigate('/')}
            className="px-4 py-2 text-sm font-medium text-gray-700 hover:text-gray-900"
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  )
}
