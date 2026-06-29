import { useState, useEffect, useRef } from 'react';
import Settings from './Settings';
import './Sidebar.css';

export default function Sidebar({
  conversations,
  currentConversationId,
  onSelectConversation,
  onNewConversation,
  onDeleteConversation,
  onRenameConversation,
}) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [editingId, setEditingId] = useState(null);
  const [editingTitle, setEditingTitle] = useState('');
  const editInputRef = useRef(null);

  useEffect(() => {
    if (editingId && editInputRef.current) {
      editInputRef.current.focus();
      editInputRef.current.select();
    }
  }, [editingId]);

  function handleDelete(e, id) {
    e.stopPropagation();
    if (window.confirm('确认删除此对话？')) {
      onDeleteConversation(id);
    }
  }

  function startRename(e, conv) {
    e.stopPropagation();
    setEditingId(conv.id);
    setEditingTitle(conv.title || '新对话');
  }

  function commitRename() {
    if (editingId === null) {
      return;
    }
    const id = editingId;
    const title = editingTitle.trim();
    setEditingId(null);
    setEditingTitle('');
    if (title) {
      onRenameConversation?.(id, title);
    }
  }

  function cancelRename() {
    setEditingId(null);
    setEditingTitle('');
  }

  function handleEditKeyDown(e) {
    if (e.key === 'Enter') {
      e.preventDefault();
      commitRename();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      cancelRename();
    }
  }

  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <h1>大模型委员会</h1>
        <button className="new-conversation-btn" onClick={onNewConversation}>
          + 新建对话
        </button>
      </div>

      <div className="conversation-list">
        {conversations.length === 0 ? (
          <div className="no-conversations">暂无对话记录</div>
        ) : (
          conversations.map((conv) => (
            <div
              key={conv.id}
              className={`conversation-item ${
                conv.id === currentConversationId ? 'active' : ''
              }`}
              onClick={() => {
                if (editingId !== conv.id) {
                  onSelectConversation(conv.id);
                }
              }}
            >
              <div className="conversation-title-row">
                {editingId === conv.id ? (
                  <input
                    ref={editInputRef}
                    className="conversation-title-input"
                    value={editingTitle}
                    onChange={(e) => setEditingTitle(e.target.value)}
                    onKeyDown={handleEditKeyDown}
                    onBlur={commitRename}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <div
                    className="conversation-title"
                    onDoubleClick={(e) => startRename(e, conv)}
                    title="双击重命名"
                  >
                    {conv.title || '新对话'}
                  </div>
                )}
                {editingId !== conv.id && (
                  <div className="conversation-actions">
                    <button
                      className="conversation-rename-btn"
                      onClick={(e) => startRename(e, conv)}
                      title="重命名对话"
                    >
                      ✎
                    </button>
                    <button
                      className="conversation-delete-btn"
                      onClick={(e) => handleDelete(e, conv.id)}
                      title="删除对话"
                    >
                      ×
                    </button>
                  </div>
                )}
              </div>
              <div className="conversation-meta">
                {conv.message_count} 条消息
              </div>
            </div>
          ))
        )}
      </div>

      <div className="sidebar-footer">
        <button className="settings-btn" onClick={() => setSettingsOpen(true)}>
          <span className="settings-btn-icon">⚙</span>
          设置
        </button>
      </div>

      <Settings isOpen={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}