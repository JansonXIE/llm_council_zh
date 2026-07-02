import { useState, useEffect, useRef } from 'react';
import ReactMarkdown from 'react-markdown';
import Stage1 from './Stage1';
import Stage2 from './Stage2';
import Stage3 from './Stage3';
import './ChatInterface.css';

export default function ChatInterface({
  conversation,
  onSendMessage,
  isLoading,
  councilEnabled = true,
  onToggleCouncil,
}) {
  const [input, setInput] = useState('');
  const [images, setImages] = useState([]);
  const [files, setFiles] = useState([]);
  const messagesEndRef = useRef(null);
  const fileInputRef = useRef(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [conversation]);

  const readFileAsDataUrl = (file) =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });

  const addImageFiles = async (files) => {
    const imageFiles = Array.from(files || []).filter((f) =>
      f.type.startsWith('image/')
    );
    if (imageFiles.length === 0) return;
    try {
      const dataUrls = await Promise.all(imageFiles.map(readFileAsDataUrl));
      setImages((prev) => [...prev, ...dataUrls]);
    } catch (err) {
      console.error('读取图片失败:', err);
    }
  };

  const addPdfFiles = async (fileList) => {
    const pdfFiles = Array.from(fileList || []).filter(
      (f) => f.type === 'application/pdf' || f.name.toLowerCase().endsWith('.pdf')
    );
    if (pdfFiles.length === 0) return;
    try {
      const entries = await Promise.all(
        pdfFiles.map(async (f) => ({
          name: f.name,
          data: await readFileAsDataUrl(f),
        }))
      );
      setFiles((prev) => [...prev, ...entries]);
    } catch (err) {
      console.error('读取 PDF 失败:', err);
    }
  };

  // Handles both images and PDFs from a single file input.
  const handleFilesSelected = async (e) => {
    const selected = e.target.files;
    await Promise.all([addImageFiles(selected), addPdfFiles(selected)]);
    // Reset the input so the same file can be selected again later.
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  const handlePaste = async (e) => {
    const items = e.clipboardData?.items;
    if (!items) return;
    const pastedImages = [];
    const pastedPdfs = [];
    for (const item of items) {
      if (item.kind !== 'file') continue;
      const file = item.getAsFile();
      if (!file) continue;
      if (item.type.startsWith('image/')) {
        pastedImages.push(file);
      } else if (
        item.type === 'application/pdf' ||
        file.name.toLowerCase().endsWith('.pdf')
      ) {
        pastedPdfs.push(file);
      }
    }
    if (pastedImages.length > 0 || pastedPdfs.length > 0) {
      // Prevent the (usually empty) placeholder text from being inserted.
      e.preventDefault();
      await Promise.all([addImageFiles(pastedImages), addPdfFiles(pastedPdfs)]);
    }
  };

  const handleRemoveImage = (index) => {
    setImages((prev) => prev.filter((_, i) => i !== index));
  };

  const handleRemoveFile = (index) => {
    setFiles((prev) => prev.filter((_, i) => i !== index));
  };

  const handleSubmit = (e) => {
    e.preventDefault();
    const hasContent = input.trim() || images.length > 0 || files.length > 0;
    if (hasContent && !isLoading) {
      onSendMessage(input, images, files);
      setInput('');
      setImages([]);
      setFiles([]);
    }
  };

  const handleKeyDown = (e) => {
    // Submit on Enter (without Shift)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  if (!conversation) {
    return (
      <div className="chat-interface">
        <div className="empty-state">
          <h2>欢迎使用大模型委员会</h2>
          <p>请在左侧新建一个对话以开始</p>
        </div>
      </div>
    );
  }

  return (
    <div className="chat-interface">
      <div className="messages-container">
        {conversation.messages.length === 0 ? (
          <div className="empty-state">
            <h2>开始新对话</h2>
            <p>在下方输入问题向模型发起咨询</p>
          </div>
        ) : (
          conversation.messages.map((msg, index) => (
            <div key={index} className="message-group">
              {msg.role === 'user' ? (
                <div className="user-message">
                  <div className="message-label">您</div>
                  <div className="message-content">
                    {msg.images && msg.images.length > 0 && (
                      <div className="message-images">
                        {msg.images.map((img, i) => (
                          <img
                            key={i}
                            src={img}
                            alt={`附图 ${i + 1}`}
                            className="message-image"
                          />
                        ))}
                      </div>
                    )}
                    {msg.files && msg.files.length > 0 && (
                      <div className="message-files">
                        {msg.files.map((file, i) => (
                          <div key={i} className="message-file">
                            <span className="message-file-icon">📄</span>
                            <span className="message-file-name">
                              {file.name || `文档 ${i + 1}`}
                            </span>
                          </div>
                        ))}
                      </div>
                    )}
                    {msg.content && (
                      <div className="markdown-content">
                        <ReactMarkdown>{msg.content}</ReactMarkdown>
                      </div>
                    )}
                  </div>
                </div>
              ) : (
                <div className="assistant-message">
                  <div className="message-label">大模型委员会</div>

                  {/* Stage 1 */}
                  {msg.loading?.stage1 && (
                    <div className="stage-loading">
                      <div className="spinner"></div>
                      <span>阶段 1：正在收集各模型的回答...</span>
                    </div>
                  )}
                  {msg.stage1 && msg.stage1.length > 0 && <Stage1 responses={msg.stage1} />}

                  {/* Stage 2 */}
                  {msg.loading?.stage2 && (
                    <div className="stage-loading">
                      <div className="spinner"></div>
                      <span>阶段 2：模型交叉评分与评价...</span>
                    </div>
                  )}
                  {msg.stage2 && msg.stage2.length > 0 && (
                    <Stage2
                      rankings={msg.stage2}
                      labelToModel={msg.metadata?.label_to_model}
                      aggregateRankings={msg.metadata?.aggregate_rankings}
                    />
                  )}

                  {/* Stage 3 */}
                  {msg.loading?.stage3 && (
                    <div className="stage-loading">
                      <div className="spinner"></div>
                      <span>
                        {msg.stage1 && msg.stage1.length === 0
                          ? '主席模型正在直接回答...'
                          : '阶段 3：主席模型总结最终答案...'}
                      </span>
                    </div>
                  )}
                  {msg.stage3 && <Stage3 finalResponse={msg.stage3} />}
                </div>
              )}
            </div>
          ))
        )}

        {isLoading && (
          <div className="loading-indicator">
            <div className="spinner"></div>
            <span>正在咨询模型...</span>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      <form className="input-form" onSubmit={handleSubmit}>
        <div className="council-toggle-bar">
          <label className="council-toggle">
            <span className="council-toggle-label">大模型委员会</span>
            <span className="toggle-switch">
              <input
                type="checkbox"
                checked={councilEnabled}
                disabled={isLoading}
                onChange={(e) => onToggleCouncil?.(e.target.checked)}
              />
              <span className="toggle-slider"></span>
            </span>
          </label>
          <span className="council-toggle-hint">
            {councilEnabled
              ? '完整三阶段：回答 → 互评 → 综合'
              : '已关闭，直接由主席模型（Chairman）回答'}
          </span>
        </div>
        {images.length > 0 && (
          <div className="image-preview-bar">
            {images.map((img, i) => (
              <div key={i} className="image-preview-item">
                <img src={img} alt={`预览 ${i + 1}`} className="image-preview-thumb" />
                <button
                  type="button"
                  className="image-preview-remove"
                  onClick={() => handleRemoveImage(i)}
                  aria-label="移除图片"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        )}
        {files.length > 0 && (
          <div className="file-preview-bar">
            {files.map((file, i) => (
              <div key={i} className="file-preview-item">
                <span className="file-preview-icon">📄</span>
                <span className="file-preview-name" title={file.name}>
                  {file.name || `文档 ${i + 1}`}
                </span>
                <button
                  type="button"
                  className="file-preview-remove"
                  onClick={() => handleRemoveFile(i)}
                  aria-label="移除文件"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        )}
        <textarea
          className="message-input"
          placeholder="输入您的问题... (Shift+Enter 换行, Enter 发送, Ctrl+V 粘贴图片/PDF)"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          disabled={isLoading}
          rows={3}
        />
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*,application/pdf,.pdf"
          multiple
          style={{ display: 'none' }}
          onChange={handleFilesSelected}
        />
        <button
          type="button"
          className="attach-button"
          onClick={() => fileInputRef.current?.click()}
          disabled={isLoading}
          title="上传图片或 PDF"
        >
          📎 上传
        </button>
        <button
          type="submit"
          className="send-button"
          disabled={
            (!input.trim() && images.length === 0 && files.length === 0) || isLoading
          }
        >
          发送
        </button>
      </form>
    </div>
  );
}
