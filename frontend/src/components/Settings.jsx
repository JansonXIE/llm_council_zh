import { useState, useEffect } from 'react';
import { api } from '../api';
import { open } from '@tauri-apps/plugin-dialog';
import './Settings.css';

const SETTINGS_FIELDS = [
  { group: 'DeepSeek', keyPrefix: 'deepseek' },
  { group: 'MiniMax', keyPrefix: 'minimax' },
  { group: 'Kimi', keyPrefix: 'kimi' },
  { group: 'GLM', keyPrefix: 'glm' },
];

export default function Settings({ isOpen, onClose }) {
  const [settings, setSettings] = useState(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (isOpen) {
      loadSettings();
    }
  }, [isOpen]);

  async function loadSettings() {
    try {
      const result = await api.getSettings();
      setSettings(result);
      setError(null);
      setSuccess(false);
    } catch (e) {
      setError('加载设置失败: ' + e.message);
    }
  }

  function handleChange(key, value) {
    setSettings(prev => ({ ...prev, [key]: value }));
    setSuccess(false);
  }

  // Model management handlers
  function handleModelNameChange(index, newName) {
    setSettings(prev => {
      const models = [...prev.models];
      models[index] = { ...models[index], name: newName };
      return { ...prev, models };
    });
    setSuccess(false);
  }

  function handleModelActiveToggle(index) {
    setSettings(prev => {
      const models = [...prev.models];
      models[index] = { ...models[index], active: !models[index].active };
      const toggledModel = models[index];
      // If toggling off and this model is the chairman, reset chairman
      if (!toggledModel.active && prev.chairman_model === toggledModel.name) {
        const firstActive = models.find(m => m.active);
        return {
          ...prev,
          models,
          chairman_model: firstActive ? firstActive.name : '',
        };
      }
      return { ...prev, models };
    });
    setSuccess(false);
  }

  function handleAddModel() {
    setSettings(prev => ({
      ...prev,
      models: [...prev.models, { name: '', active: true }],
    }));
    setSuccess(false);
  }

  function handleRemoveModel(index) {
    setSettings(prev => {
      const removedModel = prev.models[index].name;
      const models = prev.models.filter((_, i) => i !== index);
      let chairman_model = prev.chairman_model;
      if (removedModel === chairman_model) {
        const firstActive = models.find(m => m.active);
        chairman_model = firstActive ? firstActive.name : '';
      }
      return { ...prev, models, chairman_model };
    });
    setSuccess(false);
  }

  function handleChairmanChange(value) {
    setSettings(prev => ({ ...prev, chairman_model: value }));
    setSuccess(false);
  }

  async function handlePickDataDir() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择数据保存目录',
    });
    if (selected) {
      handleChange('data_dir', selected);
    }
  }

  async function handleSave() {
    setSaving(true);
    setError(null);
    try {
      // Filter out models with empty names before saving
      const toSave = {
        ...settings,
        models: settings.models.filter(m => m.name.trim() !== ''),
      };
      await api.saveSettings(toSave);
      setSuccess(true);
      setTimeout(() => {
        setSuccess(false);
        onClose();
      }, 1200);
    } catch (e) {
      setError('保存设置失败: ' + e.message);
    } finally {
      setSaving(false);
    }
  }

  if (!isOpen || !settings) return null;

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-modal" onClick={e => e.stopPropagation()}>
        <div className="settings-header">
          <h2>设置</h2>
          <button className="settings-close-btn" onClick={onClose}>✕</button>
        </div>

        <div className="settings-body">
          {SETTINGS_FIELDS.map(({ group, keyPrefix }) => (
            <div key={keyPrefix} className="settings-group">
              <h3 className="settings-group-title">{group}</h3>
              <div className="settings-field">
                <label>API Key</label>
                <input
                  type="password"
                  value={settings[keyPrefix + '_api_key']}
                  onChange={e => handleChange(keyPrefix + '_api_key', e.target.value)}
                  placeholder="输入 API Key"
                />
              </div>
              <div className="settings-field">
                <label>Base URL</label>
                <input
                  type="text"
                  value={settings[keyPrefix + '_base_url']}
                  onChange={e => handleChange(keyPrefix + '_base_url', e.target.value)}
                  placeholder="输入 Base URL"
                />
              </div>
            </div>
          ))}

          <div className="settings-group">
            <h3 className="settings-group-title">Council Models</h3>
            <div className="settings-field">
              <label>Chairman Model</label>
              <select
                className="settings-select"
                value={settings.chairman_model}
                onChange={e => handleChairmanChange(e.target.value)}
              >
                {settings.models.filter(m => m.active).length === 0 && (
                  <option value="" disabled>No active models</option>
                )}
                {settings.models
                  .filter(m => m.active)
                  .map(m => (
                    <option key={m.name} value={m.name}>{m.name}</option>
                  ))}
              </select>
              <div className="settings-field-hint">主席模型负责综合所有议员回复生成最终答案</div>
            </div>
            {settings.models.map((model, index) => (
              <div key={index} className={`model-row ${!model.active ? 'model-dormant' : ''}`}>
                <input
                  type="text"
                  className="model-name-input"
                  value={model.name}
                  onChange={e => handleModelNameChange(index, e.target.value)}
                  placeholder="provider/model-name"
                />
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    checked={model.active}
                    onChange={() => handleModelActiveToggle(index)}
                  />
                  <span className="toggle-slider"></span>
                </label>
                <span className="model-status-label">
                  {model.active ? '在线' : '休眠'}
                </span>
                <button
                  className="model-remove-btn"
                  onClick={() => handleRemoveModel(index)}
                  title="删除模型"
                >
                  ×
                </button>
              </div>
            ))}
            <button className="model-add-btn" onClick={handleAddModel}>
              + 添加模型
            </button>
            <div className="settings-field-hint">
              切换「休眠」可临时排除模型参与讨论，不会永久删除
            </div>
          </div>

          <div className="settings-group">
            <h3 className="settings-group-title">数据存储</h3>
            <div className="settings-field">
              <label>数据保存路径</label>
              <div className="settings-input-with-btn">
                <input
                  type="text"
                  value={settings.data_dir}
                  onChange={e => handleChange('data_dir', e.target.value)}
                  placeholder="留空则使用默认路径"
                />
                <button className="pick-dir-btn" onClick={handlePickDataDir}>
                  更改
                </button>
              </div>
              <div className="settings-field-hint">留空使用应用默认数据目录，点击更改按钮选择自定义目录</div>
            </div>
          </div>
        </div>

        {error && <div className="settings-error">{error}</div>}
        {success && <div className="settings-success">设置已保存 ✓</div>}

        <div className="settings-footer">
          <button className="settings-cancel-btn" onClick={onClose}>取消</button>
          <button className="settings-save-btn" onClick={handleSave} disabled={saving}>
            {saving ? '保存中...' : '保存'}
          </button>
        </div>
      </div>
    </div>
  );
}