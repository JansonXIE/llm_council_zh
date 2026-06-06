import { useState } from 'react';
import Settings from './Settings';
import './Sidebar.css';

export default function Sidebar({
  conversations,
  currentConversationId,
  onSelectConversation,
  onNewConversation,
  onDeleteConversation,
}) {
  const [settingsOpen, setSettingsOpen] = useState(false);

  function handleDelete(e, id) {
    e.stopPropagation();
    if (window.confirm('确认删除此对话？')) {
      onDeleteConversation(id);
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
              onClick={() => onSelectConversation(conv.id)}
            >
              <div className="conversation-title-row">
                <div className="conversation-title">
                  {conv.title || '新对话'}
                </div>
                <button
                  className="conversation-delete-btn"
                  onClick={(e) => handleDelete(e, conv.id)}
                  title="删除对话"
                >
                  ×
                </button>
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