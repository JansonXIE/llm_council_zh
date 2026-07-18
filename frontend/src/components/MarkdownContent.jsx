import { useState } from 'react';
import ReactMarkdown from 'react-markdown';

function getTextContent(node) {
  if (typeof node === 'string' || typeof node === 'number') {
    return String(node);
  }

  if (Array.isArray(node)) {
    return node.map(getTextContent).join('');
  }

  if (node?.props?.children) {
    return getTextContent(node.props.children);
  }

  return '';
}

function CopyablePre({ children, ...props }) {
  const [copied, setCopied] = useState(false);
  const codeText = getTextContent(children).replace(/\n$/, '');

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(codeText);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch (err) {
      console.error('复制失败:', err);
    }
  };

  return (
    <div className="copyable-code-block">
      <button
        type="button"
        className={`copy-code-button ${copied ? 'copied' : ''}`}
        onClick={handleCopy}
        aria-label="复制代码"
      >
        {copied ? '已复制' : '复制'}
      </button>
      <pre {...props}>{children}</pre>
    </div>
  );
}

export default function MarkdownContent({ children }) {
  return (
    <ReactMarkdown components={{ pre: CopyablePre }}>
      {children}
    </ReactMarkdown>
  );
}
