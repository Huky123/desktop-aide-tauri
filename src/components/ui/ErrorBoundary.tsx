import { Component, type ErrorInfo, type ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * 全局渲染异常兜底。
 *
 * 背景：此前 main.tsx 直接渲染 App，任何一处渲染/生命周期异常都会让整个 WebView
 * 白屏，用户只能重启应用。这里捕获子树异常并给出可恢复的提示 + 重新加载按钮。
 *
 * 说明：ErrorBoundary 只能捕获渲染阶段（含子组件生命周期）异常，捕获不到事件回调
 * 与 Promise 里的异常（那些继续由各自的 try/catch 处理）。
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[ErrorBoundary] 渲染异常，已兜底避免白屏:", error, info.componentStack);
  }

  private handleReload = () => {
    window.location.reload();
  };

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="error-boundary" role="alert">
        <div className="error-boundary-title">界面出错了</div>
        <p className="error-boundary-message">
          已拦截本次异常以避免整个窗口白屏。可以点击下方按钮重新加载；若反复出现，
          请到「设置 → 通用」查看日志目录并反馈问题。
        </p>
        <p className="error-boundary-message">{error.message}</p>
        <button type="button" className="error-boundary-btn" onClick={this.handleReload}>
          重新加载
        </button>
      </div>
    );
  }
}
