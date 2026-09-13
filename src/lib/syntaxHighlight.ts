/**
 * 轻量语法高亮：单次扫描 tokenizer + 一遍上下文增强。
 *
 * 从 `components/chat/Markdown.tsx` 抽出的纯逻辑（无 React 依赖），
 * 便于单独推理与测试；组件层只负责把 token 渲染成 `<span style="color">`。
 */

export type Token = {
  type: "keyword" | "string" | "comment" | "number" | "type" | "function" | "property" | "operator" | "plain";
  value: string;
};

const KEYWORDS = new Set([
  "const", "let", "var", "function", "return", "if", "else", "for", "while",
  "do", "switch", "case", "break", "continue", "new", "this", "class",
  "extends", "import", "export", "from", "default", "async", "await",
  "try", "catch", "finally", "throw", "typeof", "instanceof", "in", "of",
  "true", "false", "null", "undefined", "void", "delete", "yield",
  "static", "get", "set", "private", "public", "protected", "readonly",
  "type", "interface", "enum", "implements", "abstract", "as", "is",
  "fn", "mut", "pub", "struct", "impl", "use", "mod", "self",
  "match", "Some", "None", "Ok", "Err", "where", "dyn", "ref", "move",
  "println", "print", "format", "vec", "String", "Option", "Result",
]);

/** 代码块头部圆点配色（按语言标识） */
export const DOT_COLORS: Record<string, string> = {
  ts: "#3178c6", tsx: "#3178c6", js: "#f7df1e", jsx: "#f7df1e",
  rs: "#dea584", rust: "#dea584", py: "#3572a5", python: "#3572a5",
  css: "#563d7c", html: "#e34f26", json: "#292929", yaml: "#cb171e",
  toml: "#9c4221", md: "#42a5f5", markdown: "#42a5f5",
  sh: "#4eaa25", bash: "#4eaa25", shell: "#4eaa25",
  sql: "#e38c00", go: "#00add8", java: "#b07219",
};

export function tokenize(code: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < code.length) {
    // 空白
    if (/\s/.test(code[i])) {
      let j = i;
      while (j < code.length && /\s/.test(code[j])) j++;
      tokens.push({ type: "plain", value: code.slice(i, j) });
      i = j;
      continue;
    }

    // 多行注释 /* ... */
    if (code[i] === "/" && code[i + 1] === "*") {
      let j = i + 2;
      while (j < code.length && !(code[j - 1] === "*" && code[j] === "/")) j++;
      if (j < code.length) j++;
      tokens.push({ type: "comment", value: code.slice(i, j) });
      i = j;
      continue;
    }

    // 单行注释 //
    if (code[i] === "/" && code[i + 1] === "/") {
      let j = i;
      while (j < code.length && code[j] !== "\n") j++;
      tokens.push({ type: "comment", value: code.slice(i, j) });
      i = j;
      continue;
    }

    // 字符串
    if (code[i] === '"' || code[i] === "'" || code[i] === "`") {
      const quote = code[i];
      let j = i + 1;
      while (j < code.length && code[j] !== quote) {
        if (code[j] === "\\") j++;
        j++;
      }
      if (j < code.length) j++;
      tokens.push({ type: "string", value: code.slice(i, j) });
      i = j;
      continue;
    }

    // 数字
    if (/\d/.test(code[i]) || (code[i] === "." && i + 1 < code.length && /\d/.test(code[i + 1]))) {
      let j = i;
      if (code[j] === ".") j++;
      while (j < code.length && /\d/.test(code[j])) j++;
      if (code[j] === "." && /\d/.test(code[j + 1])) {
        j++;
        while (j < code.length && /\d/.test(code[j])) j++;
      }
      tokens.push({ type: "number", value: code.slice(i, j) });
      i = j;
      continue;
    }

    // 标识符 / 关键字
    if (/[a-zA-Z_$]/.test(code[i])) {
      let j = i;
      while (j < code.length && /[\w$]/.test(code[j])) j++;
      const word = code.slice(i, j);
      tokens.push({ type: KEYWORDS.has(word) ? "keyword" : "plain", value: word });
      i = j;
      continue;
    }

    // 其余字符（运算符、标点等）
    tokens.push({ type: "plain", value: code[i] });
    i++;
  }

  return tokens;
}

/** token 类型 → CSS 颜色变量 */
export const TOKEN_COLORS: Record<Token["type"], string> = {
  keyword:  "var(--syntax-keyword)",
  string:   "var(--syntax-string)",
  comment:  "var(--text-tertiary)",
  number:   "var(--syntax-number)",
  type:     "var(--syntax-type)",
  function: "var(--syntax-function)",
  property: "var(--syntax-property)",
  operator: "var(--syntax-operator)",
  plain:    "",
};

/** 操作符集合 — 用于第二遍识别 */
const OPERATORS = new Set([
  "=>", "==", "!=", "===", "!==", ">=", "<=", "&&", "||",
  "::", "->", "&", "|", "^", "~", "<<", ">>", "?", ":",
  "+=", "-=", "*=", "/=", "%=", "**",
]);

/**
 * 第二遍增强：在语法上下文中升级 plain token 的类型
 * - 首字母大写 → type（类/接口）
 * - 后跟 ( → function（函数调用）
 * - 前一个非空白 token 是 . → property（属性访问）
 * - 匹配操作符 → operator
 */
export function enhanceTokens(tokens: Token[], source: string): Token[] {
  const enhanced = [...tokens];

  // 计算每个 token 在源码中的起始位置
  let pos = 0;
  const positions = tokens.map((t) => {
    const start = pos;
    pos += t.value.length;
    return start;
  });

  for (let i = 0; i < enhanced.length; i++) {
    const t = enhanced[i];
    if (t.type !== "plain") continue;

    const word = t.value;
    const start = positions[i];
    const end = start + word.length;

    // 操作符检测（单字符或多字符）
    if (OPERATORS.has(word)) {
      enhanced[i] = { type: "operator", value: word };
      continue;
    }

    // 属性访问：前一个非空白 token 是 .
    let prevNonSpace: Token | null = null;
    for (let j = i - 1; j >= 0; j--) {
      if (enhanced[j].value.trim() !== "") {
        prevNonSpace = enhanced[j];
        break;
      }
    }
    if (prevNonSpace && prevNonSpace.value === ".") {
      enhanced[i] = { type: "property", value: word };
      continue;
    }

    // 函数调用：后跟 (
    const after = source.slice(end).trimStart();
    if (after.startsWith("(")) {
      enhanced[i] = { type: "function", value: word };
      continue;
    }

    // 类型名：首字母大写
    if (/^[A-Z]/.test(word)) {
      enhanced[i] = { type: "type", value: word };
    }
  }

  return enhanced;
}
