import { useState, useEffect } from 'react';
import { api } from '../api';
import { open, ask, message } from '@tauri-apps/plugin-dialog';
import { check } from '@tauri-apps/plugin-updater';
import { getVersion } from '@tauri-apps/api/app';
import './Settings.css';

const API_FORMAT_OPTIONS = [
  { value: 'openai', label: 'OpenAI Compatible' },
  { value: 'anthropic_messages', label: 'Anthropic Messages' },
  { value: 'gemini_messages', label: 'Gemini generateContent' },
];

export default function Settings({ isOpen, onClose }) {
  const [settings, setSettings] = useState(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(false);
  const [appVersion, setAppVersion] = useState('');
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateStatus, setUpdateStatus] = useState(null);

  useEffect(() => {
    if (isOpen) {
      loadSettings();
      fetchAppVersion();
    }
  }, [isOpen]);

  async function fetchAppVersion() {
    try {
      const version = await getVersion();
      setAppVersion(version);
    } catch (e) {
      console.error('Failed to get app version:', e);
    }
  }

  async function handleCheckUpdate() {
    setCheckingUpdate(true);
    setUpdateStatus(null);
    try {
      const update = await check();
      if (update) {
        setUpdateStatus({ type: 'available', text: `发现新版本 v${update.version}` });
        const yes = await ask(`发现新版本 v${update.version}\n\n是否立即下载并安装？`, {
          title: '发现新版本',
          kind: 'info',
        });
        if (yes) {
          await update.downloadAndInstall();
          await message('更新包下载并安装完成，请重新启动应用以应用更新。', { title: '更新成功', kind: 'info' });
        }
      } else {
        setUpdateStatus({ type: 'latest', text: '当前已是最新版本' });
      }
    } catch (e) {
      // 把真实错误暴露出来，便于排查（签名 / URL / 网络等问题）。
      const detail = e && e.message ? e.message : String(e);
      console.error('Manual update check failed:', e);
      setUpdateStatus({ type: 'error', text: `检查更新失败: ${detail}` });
    } finally {
      setCheckingUpdate(false);
    }
  }

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
  function handleModelFieldChange(index, field, value) {
    setSettings(prev => {
      const models = [...prev.models];
      const previousName = models[index].name;
      models[index] = { ...models[index], [field]: value };
      if (field === 'name' && prev.chairman_model === previousName) {
        return { ...prev, models, chairman_model: value };
      }
      return { ...prev, models };
    });
    setSuccess(false);
  }

  function handleModelActiveToggle(index) {
    setSettings(prev => {
      const models = [...prev.models];
      models[index] = { ...models[index], active: !models[index].active };
      // A dormant model can still serve as chairman, so we no longer reset it here.
      return { ...prev, models };
    });
    setSuccess(false);
  }

  function handleAddModel() {
    setSettings(prev => ({
      ...prev,
      models: [...prev.models, { name: '', api_key: '', base_url: '', api_format: 'openai', active: true }],
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

  function handleImageModelChange(value) {
    setSettings(prev => ({ ...prev, image_model: value }));
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
          <div className="settings-group">
            <h3 className="settings-group-title">Council Models</h3>
            <div className="settings-field">
              <label>Chairman Model</label>
              <select
                className="settings-select"
                value={settings.chairman_model}
                onChange={e => handleChairmanChange(e.target.value)}
              >
                {settings.models.filter(m => m.name.trim()).length === 0 && (
                  <option value="" disabled>No models configured</option>
                )}
                {/* If the saved chairman no longer matches any model, surface it so the user can fix it. */}
                {settings.chairman_model.trim() &&
                  !settings.models.some(m => m.name.trim() === settings.chairman_model.trim()) && (
                    <option value={settings.chairman_model}>
                      {settings.chairman_model}（已失效，请重新选择）
                    </option>
                  )}
                {settings.models
                  .filter(m => m.name.trim())
                  .map(m => (
                    <option key={m.name} value={m.name}>
                      {m.name}{m.active ? '' : '（休眠）'}
                    </option>
                  ))}
              </select>
              <div className="settings-field-hint">主席模型负责综合所有模型回复生成最终答案</div>
            </div>
            <div className="settings-field">
              <label>图片分析模型</label>
              <select
                className="settings-select"
                value={settings.image_model || ''}
                onChange={e => handleImageModelChange(e.target.value)}
              >
                <option value="">未设置（不支持图片分析）</option>
                {/* If the saved image model no longer matches any model, surface it so the user can fix it. */}
                {settings.image_model && settings.image_model.trim() &&
                  !settings.models.some(m => m.name.trim() === settings.image_model.trim()) && (
                    <option value={settings.image_model}>
                      {settings.image_model}（已失效，请重新选择）
                    </option>
                  )}
                {settings.models
                  .filter(m => m.name.trim())
                  .map(m => (
                    <option key={m.name} value={m.name}>
                      {m.name}{m.active ? '' : '（休眠）'}
                    </option>
                  ))}
              </select>
              <div className="settings-field-hint">图片分析模型用于直接分析图片并回答，需选择支持视觉（多模态）的模型</div>
            </div>
            {settings.models.map((model, index) => (
              <div key={index} className={`model-card ${!model.active ? 'model-dormant' : ''}`}>
                <div className="model-card-header">
                  <input
                    type="text"
                    className="model-name-input"
                    value={model.name}
                    onChange={e => handleModelFieldChange(index, 'name', e.target.value)}
                    placeholder="模型名称，例如 deepseek-chat"
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
                <div className="model-card-fields">
                  <div className="model-field">
                    <label>请求格式</label>
                    <select
                      className="model-format-select"
                      value={model.api_format || 'openai'}
                      onChange={e => handleModelFieldChange(index, 'api_format', e.target.value)}
                    >
                      {API_FORMAT_OPTIONS.map(option => (
                        <option key={option.value} value={option.value}>{option.label}</option>
                      ))}
                    </select>
                  </div>
                  <div className="model-field">
                    <label>API Key</label>
                    <input
                      type="password"
                      value={model.api_key}
                      onChange={e => handleModelFieldChange(index, 'api_key', e.target.value)}
                      placeholder="输入该模型的 API Key"
                    />
                  </div>
                  <div className="model-field">
                    <label>Base URL</label>
                    <input
                      type="text"
                      value={model.base_url}
                      onChange={e => handleModelFieldChange(index, 'base_url', e.target.value)}
                      placeholder="例如 https://api.sophnet.com/v1"
                    />
                  </div>
                </div>
              </div>
            ))}
            <button className="model-add-btn" onClick={handleAddModel}>
              + 添加模型
            </button>
            <div className="settings-field-hint">
              每个模型需填写名称、API Key、Base URL 和请求格式。SophNet Anthropic 可填 https://api.sophnet.com/v1，Gemini 可填完整 generateContent 地址
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

          <div className="settings-group">
            <h3 className="settings-group-title">关于与更新</h3>
            <div className="settings-field" style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <label style={{ marginBottom: 0 }}>当前版本</label>
              <span style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <span style={{ color: '#666', fontSize: '14px' }}>{appVersion ? `v${appVersion}` : '...'}</span>
                <button
                  type="button"
                  className="settings-cancel-btn"
                  onClick={handleCheckUpdate}
                  disabled={checkingUpdate}
                  style={{ padding: '4px 12px', fontSize: '13px' }}
                >
                  {checkingUpdate ? '检查中...' : '检查更新'}
                </button>
              </span>
            </div>
            {updateStatus && (
              <div
                className="settings-field-hint"
                style={{
                  marginTop: '8px',
                  color: updateStatus.type === 'error' ? '#d9534f' : updateStatus.type === 'available' ? '#4a90e2' : '#5cb85c',
                  wordBreak: 'break-all',
                }}
              >
                {updateStatus.text}
              </div>
            )}
            <div className="settings-field" style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: '12px' }}>
              <label style={{ marginBottom: 0 }}>自动检查更新</label>
              <label className="toggle-switch">
                <input
                  type="checkbox"
                  checked={settings.auto_update !== false}
                  onChange={e => handleChange('auto_update', e.target.checked)}
                />
                <span className="toggle-slider"></span>
              </label>
            </div>
            <div className="settings-field-hint">启动时自动检查是否有新版本可用</div>
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