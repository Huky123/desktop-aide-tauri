/**
 * 二选一分段控件：用于「识图 / 出图」这类只有两个选项的设置。
 *
 * 两个选项始终有一个处于选中态（值非法时回落到第一项），
 * 这样界面上不会出现"两个按钮都没亮"的困惑状态。
 */
export function BinaryChoice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly { value: T; label: string; desc: string }[];
  onChange: (value: T) => void;
}) {
  const active = options.find((option) => option.value === value) ?? options[0];
  return (
    <div>
      <p className="text-[11px] mb-1" style={{ color: "var(--text-secondary)" }}>
        {label}
      </p>
      <div className="grid grid-cols-2 gap-1.5" role="group" aria-label={label}>
        {options.map((option) => {
          const selected = option.value === active.value;
          return (
            <button
              key={option.value}
              type="button"
              aria-pressed={selected}
              onClick={() => onChange(option.value)}
              className="px-2.5 py-1.5 rounded-md text-[11px] text-center transition-colors"
              style={{
                background: selected
                  ? "color-mix(in srgb, var(--accent) 16%, var(--surface-active))"
                  : "var(--surface-active)",
                border: `${selected ? 2 : 1}px solid`,
                borderColor: selected ? "var(--accent)" : "var(--border)",
                color: selected ? "var(--text-primary)" : "var(--text-secondary)",
                fontWeight: selected ? 600 : 400,
              }}
            >
              {option.label}
            </button>
          );
        })}
      </div>
      <p className="settings-option-desc">{active.desc}</p>
    </div>
  );
}
