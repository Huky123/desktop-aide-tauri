import { useState } from "react";
import type { InputHTMLAttributes } from "react";

interface FormFieldProps {
  label: string;
  hint?: string;
  value: string | number;
  onChange: (value: string) => void;
  type?: "text" | "password" | "number" | "color";
  placeholder?: string;
  min?: number;
  max?: number;
  step?: number | string;
  className?: string;
  inputProps?: InputHTMLAttributes<HTMLInputElement>;
}

/** 封装 label + input + focus/blur 行为，消除 SettingsPanel 中重复的 inline style 模式 */
export function FormField({
  label,
  hint,
  value,
  onChange,
  type = "text",
  placeholder,
  min,
  max,
  step,
  className = "",
  inputProps,
}: FormFieldProps) {
  const [showPassword, setShowPassword] = useState(false);
  const actualType = type === "password" && showPassword ? "text" : type;

  return (
    <div className={className}>
      <label
        className="text-[11px] mb-1 block"
        style={{ color: "var(--text-secondary)" }}
      >
        {label}
        {hint && (
          <span style={{ color: "var(--text-tertiary)" }}> {hint}</span>
        )}
      </label>
      <div className="form-field-control">
        <input
          {...inputProps}
          type={actualType}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          min={min}
          max={max}
          step={step}
          className={`input-field w-full text-sm px-2.5 py-1.5${type === "password" ? " pr-9" : ""}${inputProps?.className ? ` ${inputProps.className}` : ""}`}
        />
        {type === "password" && (
          <button
            type="button"
            className="form-field-password-toggle"
            onClick={() => setShowPassword((visible) => !visible)}
            aria-label={showPassword ? "隐藏 API Key" : "显示 API Key"}
            title={showPassword ? "隐藏 API Key" : "显示 API Key"}
          >
            {showPassword ? (
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 3l18 18M10.6 10.7a2 2 0 0 0 2.7 2.7M9.9 4.2A10.7 10.7 0 0 1 12 4c5.5 0 9 5.5 9 8a8.8 8.8 0 0 1-2 3.4M6.6 6.7C4.3 8.2 3 10.5 3 12c0 2.5 3.5 8 9 8 1.4 0 2.7-.4 3.8-1" /></svg>
            ) : (
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12c0-2.5 3.5-8 9-8s9 5.5 9 8-3.5 8-9 8-9-5.5-9-8Z" /><circle cx="12" cy="12" r="3" /></svg>
            )}
          </button>
        )}
      </div>
    </div>
  );
}
