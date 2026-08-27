import { heicTo } from "heic-to/csp";

interface DecodeRequest {
  readonly file: File;
  readonly expectedWidth: number;
  readonly expectedHeight: number;
  readonly maxCompressedBytes: number;
  readonly maxPixels: number;
}

type DecodeResponse =
  | { readonly ok: true; readonly blob: Blob; readonly width: number; readonly height: number }
  | { readonly ok: false; readonly message: string };

self.onmessage = async (event: MessageEvent<DecodeRequest>) => {
  const { file, expectedWidth, expectedHeight, maxCompressedBytes, maxPixels } = event.data;
  try {
    const expectedPixels = expectedWidth * expectedHeight;
    if (
      !Number.isSafeInteger(expectedPixels) ||
      expectedWidth <= 0 ||
      expectedHeight <= 0 ||
      expectedPixels > maxPixels
    ) {
      throw new Error("The HEIC dimensions exceed the browser decoder allocation guard.");
    }
    if (file.size > maxCompressedBytes) {
      throw new Error("The compressed HEIC exceeds the browser decoder input guard.");
    }
    const bitmap = await heicTo({ blob: file, type: "bitmap" });
    const actualWidth = bitmap.width;
    const actualHeight = bitmap.height;
    if (actualWidth !== expectedWidth || actualHeight !== expectedHeight) {
      bitmap.close();
      throw new Error(
        `The decoded HEIC dimensions ${actualWidth}×${actualHeight} do not match ${expectedWidth}×${expectedHeight}.`,
      );
    }
    const canvas = new OffscreenCanvas(actualWidth, actualHeight);
    let bitmapClosed = false;
    try {
      const context = canvas.getContext("2d");
      if (!context) throw new Error("The browser cannot allocate a lossless HEIC display canvas.");
      context.drawImage(bitmap, 0, 0);
      bitmap.close();
      bitmapClosed = true;
      const blob = await canvas.convertToBlob({ type: "image/png" });
      const response: DecodeResponse = {
        ok: true,
        blob,
        width: expectedWidth,
        height: expectedHeight,
      };
      self.postMessage(response);
    } finally {
      if (!bitmapClosed) bitmap.close();
      canvas.width = 1;
      canvas.height = 1;
    }
  } catch (error) {
    const response: DecodeResponse = {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
    };
    self.postMessage(response);
  }
};
