import { describe, expect, it } from "vitest";
import { DOT_COLORS, enhanceTokens, tokenize } from "./syntaxHighlight";

/** 把 token 还原成源码——高亮不得增删任何字符 */
function render(tokens: { value: string }[]): string {
  return tokens.map((t) => t.value).join("");
}

const SAMPLES = [
  'const a = 1;\n// 行注释\nlet s = "文本\\n转义";',
  "/* 多行\n注释 */\nfunction f(x) { return x.y(1.5) + 0b1; }",
  "impl Foo { fn bar(&self) -> Result<(), Error> { Ok(()) } }",
  "```\n`模板 ${x}`\n```",
  "",
  "纯中文没有代码",
];

describe("tokenize / enhanceTokens", () => {
  it("无损往返：token 拼接结果与源码逐字符一致", () => {
    for (const source of SAMPLES) {
      expect(render(tokenize(source))).toBe(source);
      expect(render(enhanceTokens(tokenize(source), source))).toBe(source);
    }
  });

  it("识别关键字、字符串、注释、数字", () => {
    const types = (code: string) => tokenize(code).map((t) => t.type);
    expect(types("const x")).toEqual(["keyword", "plain", "plain"]);
    expect(types('"hi"')).toEqual(["string"]);
    expect(types("// note")).toEqual(["comment"]);
    expect(types("42")).toEqual(["number"]);
    expect(types("3.14")).toEqual(["number"]);
  });

  it("未闭合的字符串/多行注释也消费到结尾，不丢字符", () => {
    expect(render(tokenize("const s = \"未闭合"))).toBe("const s = \"未闭合");
    expect(tokenize("const s = \"未闭合").at(-1)?.type).toBe("string");
    expect(tokenize("/* 未闭合").at(-1)?.type).toBe("comment");
  });

  it("第二遍增强：点号后是属性、括号前是函数、仅首字母大写是类型", () => {
    const source = "obj.method(); Bar(); Baz";
    const enhanced = enhanceTokens(tokenize(source), source);
    const typed = new Map(enhanced.map((t) => [t.value, t.type]));
    expect(typed.get("method")).toBe("property");
    // 判定顺序是 属性 → 函数 → 类型：后跟 ( 的优先级高于首字母大写
    expect(typed.get("Bar")).toBe("function");
    expect(typed.get("Baz")).toBe("type");
  });

  it("只有 OPERATORS 集合内的单字符运算符会被标为 operator", () => {
    const source = "a & b : c + d";
    const typed = new Map(
      enhanceTokens(tokenize(source), source).map((t) => [t.value, t.type]),
    );
    expect(typed.get("&")).toBe("operator");
    expect(typed.get(":")).toBe("operator");
    // `+` 不在集合内 → 保持 plain
    expect(typed.get("+")).toBe("plain");
  });

  /**
   * 记录现状（非期望行为）：tokenize 对手写运算符不做合并，
   * 因此 `OPERATORS` 里的多字符条目（"=>"、"=="、"&&" 等）永远匹配不到——
   * 它们会被拆成逐个单字符 token。若要改进高亮质量，需让 tokenize 合并运算符串。
   */
  it("多字符运算符会被拆成单字符 token（当前实现的已知限制）", () => {
    const source = "a => b";
    const tokens = tokenize(source);
    expect(tokens.map((t) => t.value)).toContain("=");
    expect(tokens.map((t) => t.value)).toContain(">");
    expect(tokens.map((t) => t.value)).not.toContain("=>");
  });

  it("关键字不会被第二遍增强降级为 plain", () => {
    const source = "const x = 1";
    const enhanced = enhanceTokens(tokenize(source), source);
    expect(enhanced[0]).toEqual({ type: "keyword", value: "const" });
  });

  it("语言圆点配色覆盖常见语言，未知语言由调用方兜底", () => {
    expect(DOT_COLORS.ts).toBeTruthy();
    expect(DOT_COLORS.rs).toBeTruthy();
    expect(DOT_COLORS.unknown).toBeUndefined();
  });
});
