import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Markdown } from "./Markdown";

describe("Markdown 图片", () => {
  it("本地保存的 AI 图片可以打开灯箱", () => {
    const onImageClick = vi.fn();
    render(
      <Markdown
        content="![生成的图片](http://asset.localhost/generated.png)"
        onImageClick={onImageClick}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "生成的图片" }));

    expect(onImageClick).toHaveBeenCalledWith(expect.objectContaining({
      data: "",
      url: "http://asset.localhost/generated.png",
      mimeType: "image/png",
    }));
  });
});
