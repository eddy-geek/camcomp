import { describe, expect, it, vi } from "vitest";

import { validPhoto } from "./fixtures";
import {
  BrowserSourceResolver,
  classifyBrowserFiles,
  HighMemoryHeicConfirmationRequired,
  probeHeifDimensions,
  resolveHeicPhotoSource,
  UnsupportedHeicError,
} from "../src/sources";

function file(name: string, bytes: BlobPart[] = ["abc"], relativePath = ""): File {
  const value = new File(bytes, name);
  if (relativePath) Object.defineProperty(value, "webkitRelativePath", { value: relativePath });
  return value;
}

function heicFile(name = "a.heic"): File {
  const bytes = new Uint8Array(24);
  bytes.set([0, 0, 0, 24], 0);
  bytes.set(
    [..."ftypheic"].map((character) => character.charCodeAt(0)),
    4,
  );
  return file(name, [bytes]);
}

describe("browser source resolution", () => {
  it("reads encoded HEIF dimensions before allocating the decoder", async () => {
    const box = new Uint8Array(20);
    const view = new DataView(box.buffer);
    view.setUint32(0, 20, false);
    box.set(
      [..."ispe"].map((character) => character.charCodeAt(0)),
      4,
    );
    view.setUint32(12, 8192, false);
    view.setUint32(16, 6144, false);
    await expect(probeHeifDimensions(new Blob([box]))).resolves.toEqual({
      width: 8192,
      height: 6144,
    });
  });

  it("classifies one JSON plus photos and rejects multiple JSON documents", () => {
    const classified = classifyBrowserFiles([file("camcomp.json"), file("a.jpg")]);
    expect(classified.kind).toBe("analysis-with-files");
    expect(classified.analysisFile?.name).toBe("camcomp.json");
    expect(classified.sourceFiles.map(({ path }) => path)).toEqual(["a.jpg"]);
    expect(() => classifyBrowserFiles([file("one.json"), file("two.json")])).toThrowError(
      expect.objectContaining({ code: "multiple-analysis-files" }),
    );
  });

  it("strips the webkit directory root without weakening nested paths", () => {
    const classified = classifyBrowserFiles(
      [file("camcomp.json", [], "album/camcomp.json"), file("a.jpg", [], "album/sub/a.jpg")],
      { stripDirectoryRoot: true },
    );
    expect(classified.sourceFiles[0]?.path).toBe("sub/a.jpg");
  });

  it("releases object URLs exactly once", async () => {
    const revoked: string[] = [];
    const resolver = new BrowserSourceResolver({
      files: [{ path: "photos/a.jpg", file: file("a.jpg") }],
      createObjectUrl: () => "blob:test",
      revokeObjectUrl: (url) => revoked.push(url),
    });
    const source = await resolver.resolvePhoto(validPhoto());
    expect(source.displayFormat).toBe("jpeg");
    source.release();
    source.release();
    resolver.release();
    expect(revoked).toEqual(["blob:test"]);
  });

  it("uses only dimension-matched HEIC sidecars", async () => {
    const photo = { ...validPhoto(), path: "photos/a.heic" };
    const sidecar = file("a.jpg");
    const revoked: string[] = [];
    const resolved = await resolveHeicPhotoSource({
      photo,
      exactFile: heicFile(),
      getFile: async (path) => (path === "photos/a.jpg" ? sidecar : undefined),
      createObjectUrl: (blob) => (blob === sidecar ? "blob:sidecar" : "blob:heic"),
      revokeObjectUrl: (url) => revoked.push(url),
      decodeProbe: async () => undefined,
      dimensionProbe: async () => ({ width: 4, height: 3 }),
    });
    expect(resolved).toMatchObject({ sourceKind: "sidecar", displayFormat: "jpeg" });
    expect(revoked).toEqual(["blob:heic"]);
    resolved.release();
    expect(revoked).toEqual(["blob:heic", "blob:sidecar"]);

    await expect(
      resolveHeicPhotoSource({
        photo,
        exactFile: heicFile(),
        getFile: async () => sidecar,
        createObjectUrl: () => "blob:wrong",
        revokeObjectUrl: () => undefined,
        decodeProbe: async () => undefined,
        dimensionProbe: async () => ({ width: 3, height: 4 }),
      }),
    ).rejects.toBeInstanceOf(UnsupportedHeicError);
  });

  it("requires confirmation before the high-memory worker fallback", async () => {
    const photo = { ...validPhoto(), path: "photos/a.heic" };
    const decoded = new Blob(["lossless"], { type: "image/png" });
    const worker = vi.fn().mockResolvedValue({ blob: decoded, width: 4, height: 3 });
    const options = {
      photo,
      exactFile: heicFile(),
      getFile: async () => undefined,
      createObjectUrl: (blob: Blob) => (blob === decoded ? "blob:decoded" : "blob:heic"),
      revokeObjectUrl: vi.fn(),
      decodeProbe: async () => undefined,
      decodeWithWorker: worker,
    };
    await expect(resolveHeicPhotoSource(options)).rejects.toBeInstanceOf(
      HighMemoryHeicConfirmationRequired,
    );
    expect(worker).not.toHaveBeenCalled();

    const resolved = await resolveHeicPhotoSource({ ...options, allowHighMemoryWorker: true });
    expect(worker).toHaveBeenCalledOnce();
    expect(resolved).toMatchObject({
      sourceKind: "decoded",
      displayFormat: "png",
      displayUrl: "blob:decoded",
    });
  });
});
