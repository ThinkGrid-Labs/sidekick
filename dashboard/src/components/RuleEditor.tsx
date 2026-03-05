import type { TargetingRule, Operator } from '../types'

const OPERATORS: { value: Operator; label: string }[] = [
  { value: 'equals', label: 'equals' },
  { value: 'not_equals', label: 'does not equal' },
  { value: 'contains', label: 'contains' },
  { value: 'starts_with', label: 'starts with' },
  { value: 'ends_with', label: 'ends with' },
]

interface RuleEditorProps {
  rules: TargetingRule[]
  onChange: (rules: TargetingRule[]) => void
}

export default function RuleEditor({ rules, onChange }: RuleEditorProps) {
  function addRule() {
    onChange([...rules, { attribute: '', operator: 'equals', values: [''] }])
  }

  function removeRule(i: number) {
    onChange(rules.filter((_, idx) => idx !== i))
  }

  function updateRule(i: number, partial: Partial<TargetingRule>) {
    onChange(rules.map((r, idx) => (idx === i ? { ...r, ...partial } : r)))
  }

  function updateValues(i: number, raw: string) {
    // Comma-separated list of values
    const values = raw.split(',').map(v => v.trim()).filter(Boolean)
    updateRule(i, { values: values.length ? values : [''] })
  }

  return (
    <div className="space-y-3">
      {rules.map((rule, i) => (
        <div key={i} className="flex flex-wrap gap-2 items-start p-3 bg-gray-50 rounded-md border border-gray-200">
          <div className="flex-1 min-w-32">
            <label className="block text-xs text-gray-500 mb-1">Attribute</label>
            <input
              type="text"
              value={rule.attribute}
              onChange={e => updateRule(i, { attribute: e.target.value })}
              placeholder="e.g. email"
              className="w-full border border-gray-300 rounded px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>

          <div className="w-40">
            <label className="block text-xs text-gray-500 mb-1">Operator</label>
            <select
              value={rule.operator}
              onChange={e => updateRule(i, { operator: e.target.value as Operator })}
              className="w-full border border-gray-300 rounded px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            >
              {OPERATORS.map(op => (
                <option key={op.value} value={op.value}>{op.label}</option>
              ))}
            </select>
          </div>

          <div className="flex-1 min-w-40">
            <label className="block text-xs text-gray-500 mb-1">Values (comma-separated)</label>
            <input
              type="text"
              value={rule.values.join(', ')}
              onChange={e => updateValues(i, e.target.value)}
              placeholder="e.g. @acme.com, @example.com"
              className="w-full border border-gray-300 rounded px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>

          <div className="pt-5">
            <button
              type="button"
              onClick={() => removeRule(i)}
              className="text-red-500 hover:text-red-700 text-sm px-2 py-1.5"
              aria-label="Remove rule"
            >
              Remove
            </button>
          </div>
        </div>
      ))}

      <button
        type="button"
        onClick={addRule}
        className="text-sm text-indigo-600 hover:text-indigo-800 font-medium"
      >
        + Add rule
      </button>
    </div>
  )
}
