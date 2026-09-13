import { memo, useMemo, useCallback, useEffect, useRef, useState, type ComponentPropsWithoutRef } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import type { ImageAttachment } from "../../types";
import { DOT_COLORS, TOKEN_COLORS, enhanceTokens, tokenize } from "../../lib/syntaxHighlight";

interface MarkdownProps {
  content: string;
  /** 图片点击回调 — 用于打开灯箱查看 AI 生成的图片 */
  onImageClick?: (img: ImageAttachment) => void;
}

// ── data: URI 解析 ──

/** 解析 data: URI，提取 mimeType 和 base64 数据 */
function parseDataUri(src: string): { mimeType: string; data: string } | null {
  // 格式: data:[<mediatype>][;base64],<data>
  const match = src.match(/^data:([^;,]+)?(;base64)?,(.*)$/s);
  if (!match) return null;
  const mimeType = match[1] || "image/png";
  const isBase64 = match[2] === ";base64";
  const rawData = match[3];
  return { mimeType, data: isBase64 ? rawData : rawData };
}

const REMARK_PLUGINS = [remarkGfm, remarkMath];
const REHYPE_PLUGINS = [rehypeKatex];

/** 代码高亮组件 — 纯正则、无依赖 */
const CodeHighlight = memo(({ code, language }: { code: string; language?: string }) => {
  const tokens = useMemo(() => enhanceTokens(tokenize(code), code), [code]);
  return (
    <code className={language ? `language-${language}` : undefined}>
      {tokens.map((t, i) =>
        t.type === "plain" ? (
          t.value
        ) : (
          <span key={i} style={{ color: TOKEN_COLORS[t.type] }}>
            {t.value}
          </span>
        ),
      )}
    </code>
  );
});
CodeHighlight.displayName = "CodeHighlight";

/** 代码块组件 */
const CodeBlock = memo(({ className, children, ...props }: ComponentPropsWithoutRef<"code"> & { className?: string }) => {
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const isInline = !className;
  const language = className?.replace(/^language-/, "") || "";
  const codeText = typeof children === "string" ? children : String(children ?? "");
  const dotColor = DOT_COLORS[language] || "var(--text-tertiary)";

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(codeText).then(() => {
      setCopied(true);
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      copyTimerRef.current = setTimeout(() => setCopied(false), 1500);
    });
  }, [codeText]);

  useEffect(() => () => {
    if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
  }, []);

  if (isInline) {
    return (
      <code
        className="px-1 py-0.5 rounded text-[0.9em] font-mono"
        style={{ background: "var(--code-inline-bg)", color: "var(--code-inline-text)" }}
        {...props}
      >
        {children}
      </code>
    );
  }

  return (
    <div
      className="my-2 rounded-lg overflow-hidden"
      style={{ border: "1px solid var(--border)" }}
    >
      {language && (
        <div
          className="px-3 py-1 text-[10px] font-mono uppercase flex items-center gap-1.5"
          style={{ background: "var(--surface-active)", color: "var(--text-tertiary)" }}
        >
          <span
            className="inline-block w-1.5 h-1.5 rounded-full flex-shrink-0"
            style={{ background: dotColor }}
          />
          <span className="flex-1 truncate">{language}</span>
          <button
            onClick={handleCopy}
            className="flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors duration-150 hover:opacity-80"
            style={{
              background: "var(--surface-raised)",
              color: copied ? "var(--accent)" : "var(--text-secondary)",
              border: "1px solid var(--border)",
              cursor: "pointer",
            }}
            title="复制代码"
            aria-label="复制代码"
          >
            {copied ? (
              <>
                <svg className="w-2.5 h-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={3}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
                </svg>
                已复制
              </>
            ) : (
              <>
                <svg className="w-2.5 h-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                  <rect x="9" y="9" width="12" height="12" rx="2" />
                  <path d="M5 15V5a2 2 0 0 1 2-2h10" />
                </svg>
                复制
              </>
            )}
          </button>
        </div>
      )}
      <pre className="px-3 py-2 overflow-x-auto text-xs leading-relaxed" style={{ background: "var(--code-bg)" }}>
        <CodeHighlight code={codeText} language={language} />
      </pre>
    </div>
  );
});
CodeBlock.displayName = "CodeBlock";

/** 自定义链接 */
const A = memo(({ href, children, ...props }: ComponentPropsWithoutRef<"a">) => (
  <a
    href={href}
    target="_blank"
    rel="noopener noreferrer"
    className="underline underline-offset-2 hover:opacity-80 transition-opacity duration-150"
    style={{ color: "var(--accent)" }}
    {...props}
  >
    {children}
  </a>
));
A.displayName = "A";

// ── 自定义图片组件 ──
// 用工厂函数创建，通过闭包捕获 onImageClick，因为 react-markdown 不会透传自定义 props

function createImgComponent(onImageClick?: (img: ImageAttachment) => void) {
  return function Img({ src, alt, ...props }: ComponentPropsWithoutRef<"img">) {
    const isDataUri = src?.startsWith("data:") ?? false;
    const isInteractiveImage = Boolean(src && onImageClick);

    const handleClick = useCallback(() => {
      if (!src || !onImageClick) return;

      const parsed = isDataUri ? parseDataUri(src) : null;
      onImageClick({
        type: "image",
        id: `ai-img-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        data: parsed?.data || "",
        url: parsed ? undefined : src,
        mimeType: parsed?.mimeType || inferMimeType(src),
        name: alt || "AI 生成的图片",
        size: parsed?.data.length || 0,
        source: "ai-generated",
      });
    }, [src, isDataUri, alt]);

    return (
      <span
        className={`block my-2.5 ${isInteractiveImage ? "cursor-pointer" : ""}`}
        onClick={handleClick}
        role={isInteractiveImage ? "button" : undefined}
        tabIndex={isInteractiveImage ? 0 : undefined}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            handleClick();
          }
        }}
      >
        <img
          src={src}
          alt={alt || "图片"}
          className="max-w-full h-auto rounded-lg transition-opacity duration-150"
          style={{
            border: "1px solid var(--border)",
            maxHeight: "400px",
            objectFit: "contain",
            background: "var(--surface-hover)",
          }}
          loading="lazy"
          {...props}
        />
      </span>
    );
  };
}

function inferMimeType(src: string) {
  const path = src.split("?")[0].toLowerCase();
  if (path.endsWith(".jpg") || path.endsWith(".jpeg")) return "image/jpeg";
  if (path.endsWith(".webp")) return "image/webp";
  if (path.endsWith(".gif")) return "image/gif";
  if (path.endsWith(".svg")) return "image/svg+xml";
  return "image/png";
}

/** Markdown 渲染组件，匹配简约暗色主题 */
export const Markdown = memo(({ content, onImageClick }: MarkdownProps) => {
  // 稳定化组件引用：onImageClick 变化时重新创建 Img 组件
  const imgComponent = useMemo(
    () => createImgComponent(onImageClick),
    [onImageClick],
  );

  return (
  <ReactMarkdown
    skipHtml
    remarkPlugins={REMARK_PLUGINS}
    rehypePlugins={REHYPE_PLUGINS}
    components={{
      h1: ({ children, ...props }) => (
        <h1
          className="text-lg font-bold mt-3 mb-2 pb-1.5 first:mt-0"
          style={{ color: "var(--text-primary)", borderBottom: "1px solid var(--border)" }}
          {...props}
        >
          {children}
        </h1>
      ),
      h2: ({ children, ...props }) => (
        <h2 className="text-base font-bold mt-3 mb-1.5 first:mt-0" style={{ color: "var(--text-primary)" }} {...props}>
          <span
            style={{
              display: "inline-block",
              width: 3,
              height: 14,
              borderRadius: 2,
              background: "var(--accent)",
              marginRight: 6,
              verticalAlign: "middle",
            }}
          />
          {children}
        </h2>
      ),
      h3: ({ children, ...props }) => (
        <h3 className="text-sm font-bold mt-2 mb-1 first:mt-0" style={{ color: "var(--text-primary)" }} {...props}>
          <span
            style={{
              display: "inline-block",
              width: 2,
              height: 12,
              borderRadius: 2,
              background: "var(--accent)",
              opacity: 0.6,
              marginRight: 5,
              verticalAlign: "middle",
            }}
          />
          {children}
        </h3>
      ),
      p: ({ children, ...props }) => (
        <p className="my-1.5 leading-relaxed first:mt-0 last:mb-0" {...props}>{children}</p>
      ),
      ul: ({ children, ...props }) => (
        <ul className="list-disc list-outside pl-5 my-1.5 space-y-0.5" {...props}>{children}</ul>
      ),
      ol: ({ children, ...props }) => (
        <ol className="list-decimal list-outside pl-5 my-1.5 space-y-0.5" {...props}>{children}</ol>
      ),
      li: ({ children, ...props }) => (
        <li className="pl-0.5" style={{ color: "var(--text-secondary)" }} {...props}>{children}</li>
      ),
      blockquote: ({ children, ...props }) => (
        <blockquote
          className="pl-3 pr-3 py-2 my-2 rounded-r-md italic"
          style={{
            borderLeft: "3px solid var(--accent)",
            background: "var(--blockquote-bg)",
            color: "var(--text-secondary)",
          }}
          {...props}
        >
          {children}
        </blockquote>
      ),
      hr: (props) => <hr className="my-3" style={{ borderColor: "var(--border)" }} {...props} />,
      strong: ({ children, ...props }) => (
        <strong className="font-bold" style={{ color: "var(--text-primary)" }} {...props}>{children}</strong>
      ),
      em: ({ children, ...props }) => (
        <em className="italic" style={{ color: "var(--text-secondary)" }} {...props}>{children}</em>
      ),
      code: CodeBlock,
      a: A,
      table: ({ children, ...props }) => (
        <div className="overflow-x-auto my-2">
          <table className="min-w-full text-xs border-collapse markdown-table" {...props}>{children}</table>
        </div>
      ),
      thead: ({ children, ...props }) => (
        <thead style={{ background: "var(--surface-hover)" }} {...props}>{children}</thead>
      ),
      th: ({ children, ...props }) => (
        <th
          className="px-2 py-1.5 text-left font-medium text-[11px] uppercase tracking-wider"
          style={{ border: "1px solid var(--border)", color: "var(--text-secondary)" }}
          {...props}
        >
          {children}
        </th>
      ),
      td: ({ children, ...props }) => (
        <td className="px-2 py-1" style={{ border: "1px solid var(--border)" }} {...props}>{children}</td>
      ),
      tr: ({ children, ...props }) => (
        <tr className="transition-colors duration-150" style={{ color: "var(--text-secondary)" }} {...props}>{children}</tr>
      ),
      img: imgComponent,
    }}
  >
    {content}
  </ReactMarkdown>
  );
});

Markdown.displayName = "Markdown";
