/**
 * 将图片 Blob 缩放并转换为 base64。
 * 最长边不超过 maxDimension px，输出 JPEG quality 0.85。
 */
export function resizeImageToBase64(
  blob: Blob,
  maxDimension: number = 1568,
): Promise<{ data: string; mimeType: string; width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      let { width, height } = img;

      // 缩放至 maxDimension 以内
      if (width > maxDimension || height > maxDimension) {
        const ratio = Math.min(maxDimension / width, maxDimension / height);
        width = Math.round(width * ratio);
        height = Math.round(height * ratio);
      }

      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        reject(new Error("Canvas context 创建失败"));
        return;
      }
      ctx.drawImage(img, 0, 0, width, height);
      const data = canvas.toDataURL("image/jpeg", 0.85);
      // 去掉 "data:image/jpeg;base64," 前缀
      const base64 = data.split(",")[1] || "";
      resolve({ data: base64, mimeType: "image/jpeg", width, height });
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("图片加载失败"));
    };
    img.src = url;
  });
}
