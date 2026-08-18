import { useEffect, useState } from 'react'
import {
  addGroupMember,
  createGroup,
  deleteGroup,
  listGroupMembers,
  listGroups,
  listServers,
  removeGroupMember,
} from '../api'
import type { Group, Server } from '../types'
import { buttonStyle, deleteButtonStyle, inputStyle } from '../ui'

export function GroupsView() {
  const [groups, setGroups] = useState<Group[]>([])
  const [servers, setServers] = useState<Server[]>([])
  const [expanded, setExpanded] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)

  function refresh() {
    listGroups().then(setGroups)
  }

  useEffect(() => {
    refresh()
    listServers().then(setServers)
  }, [])

  async function handleCreate(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    try {
      await createGroup(name)
      setName('')
      refresh()
    } finally {
      setBusy(false)
    }
  }

  async function handleDelete(id: string) {
    await deleteGroup(id)
    if (expanded === id) setExpanded(null)
    refresh()
  }

  return (
    <div>
      <h1 style={{ fontSize: 20, margin: '0 0 16px' }}>Groups</h1>

      <form onSubmit={handleCreate} className="card" style={{ display: 'flex', gap: 10, marginBottom: 16 }}>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Group name"
          required
          style={{ ...inputStyle, flex: 1 }}
        />
        <button type="submit" disabled={busy} style={buttonStyle}>
          {busy ? 'Adding…' : 'Add group'}
        </button>
      </form>

      <div className="card" style={{ padding: 0 }}>
        {groups.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No groups yet.
          </div>
        ) : (
          groups.map((group) => (
            <div key={group.id} style={{ borderBottom: '1px solid var(--border)' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '12px 16px' }}>
                <button
                  onClick={() => setExpanded(expanded === group.id ? null : group.id)}
                  style={{ background: 'none', border: 'none', color: 'inherit', cursor: 'pointer', padding: 0, fontWeight: 600 }}
                >
                  {group.name}
                </button>
                <button onClick={() => handleDelete(group.id)} style={deleteButtonStyle}>
                  Delete
                </button>
              </div>
              {expanded === group.id && <GroupMembers groupId={group.id} servers={servers} />}
            </div>
          ))
        )}
      </div>
    </div>
  )
}

function GroupMembers({ groupId, servers }: { groupId: string; servers: Server[] }) {
  const [members, setMembers] = useState<Server[]>([])
  const [addId, setAddId] = useState('')

  function refresh() {
    listGroupMembers(groupId).then(setMembers)
  }

  useEffect(refresh, [groupId])

  async function handleAdd(e: React.FormEvent) {
    e.preventDefault()
    if (!addId) return
    await addGroupMember(groupId, addId)
    setAddId('')
    refresh()
  }

  async function handleRemove(serverId: string) {
    await removeGroupMember(groupId, serverId)
    refresh()
  }

  const memberIds = new Set(members.map((m) => m.id))
  const available = servers.filter((s) => !memberIds.has(s.id))

  return (
    <div style={{ padding: '0 16px 12px' }}>
      {members.length === 0 ? (
        <div className="muted" style={{ fontSize: 13, marginBottom: 8 }}>
          No members yet.
        </div>
      ) : (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: 8 }}>
          {members.map((m) => (
            <span
              key={m.id}
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 6,
                padding: '3px 8px',
                borderRadius: 999,
                border: '1px solid var(--border)',
                fontSize: 12,
              }}
            >
              {m.name}
              <button
                onClick={() => handleRemove(m.id)}
                aria-label={`Remove ${m.name}`}
                style={{ background: 'none', border: 'none', color: 'var(--text-muted)', cursor: 'pointer', padding: 0, lineHeight: 1 }}
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}
      {available.length > 0 && (
        <form onSubmit={handleAdd} style={{ display: 'flex', gap: 8 }}>
          <select value={addId} onChange={(e) => setAddId(e.target.value)} style={inputStyle}>
            <option value="">Add server…</option>
            {available.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
          <button type="submit" style={buttonStyle}>
            Add
          </button>
        </form>
      )}
    </div>
  )
}
