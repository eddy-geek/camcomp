import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import {
  HDR_GAIN_MAP_NAMESPACE,
  ISO_21496_NAMESPACE,
  ITEM_NAMESPACE,
  XMP_PACKET_SIGNATURE,
  inspectJpegGainMapUrl,
} from "../src/sources";
import {
  detectJpegGainMapRuntimeCapability,
  evaluateHdrPresentationStatus,
  presentationStatusText,
} from "../src/viewer";

function jpegSegment(marker: number, payload: Uint8Array): Uint8Array {
  const segment = new Uint8Array(payload.length + 4);
  segment.set([0xff, marker, (payload.length + 2) >> 8, (payload.length + 2) & 0xff]);
  segment.set(payload, 4);
  return segment;
}

function concat(...parts: readonly Uint8Array[]): Uint8Array {
  const result = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

function xmp(length: number): Uint8Array {
  const xml = `${XMP_PACKET_SIGNATURE}<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:hdrgm="${HDR_GAIN_MAP_NAMESPACE}" xmlns:Container="http://ns.google.com/photos/1.0/container/" xmlns:Item="${ITEM_NAMESPACE}"><rdf:RDF><rdf:Description hdrgm:Version="1.0"><Container:Directory><Container:Item Item:Semantic="Primary" Item:Mime="image/jpeg" Item:Length="10"/><Container:Item Item:Semantic="GainMap" Item:Mime="image/jpeg" Item:Length="${length}"/></Container:Directory></rdf:Description></rdf:RDF></x:xmpmeta>`;
  return new TextEncoder().encode(xml);
}

function syntheticGainMap(iso = false): Uint8Array {
  const gainMap = new Uint8Array([0xff, 0xd8, 0xff, 0x00, 0x00, 0xff, 0xd9]);
  const app2 = new TextEncoder().encode(iso ? `${ISO_21496_NAMESPACE}\0` : "MPF\0");
  const base = concat(
    new Uint8Array([0xff, 0xd8]),
    jpegSegment(0xe1, xmp(gainMap.length)),
    jpegSegment(0xe2, new TextEncoder().encode("MPF\0")),
    iso ? jpegSegment(0xe2, app2) : new Uint8Array(),
    new Uint8Array([0xff, 0xda, 0x00, 0xff, 0xd9]),
  );
  return concat(base, gainMap);
}

function syntheticPlainJpeg(): Uint8Array {
  return new Uint8Array([0xff, 0xd8, 0xff, 0xda, 0x00, 0xff, 0xd9]);
}

function rangedFetch(bytes: Uint8Array): typeof globalThis.fetch {
  return vi.fn(async (_url: RequestInfo | URL, init?: RequestInit) => {
    const range = String((init?.headers as Record<string, string> | undefined)?.Range ?? "");
    const match = /^bytes=(\d+)-(\d+)$/u.exec(range);
    const start = match ? Number(match[1]) : 0;
    const end = match ? Math.min(Number(match[2]), bytes.length - 1) : bytes.length - 1;
    const body = bytes.slice(start, end + 1);
    return new Response(body, {
      status: 206,
      headers: { "Content-Range": `bytes ${start}-${end}/${bytes.length}` },
    });
  });
}

describe("JPEG gain-map inspection", () => {
  it("recognizes Ultra HDR v1 and dual ISO signaling without decoding pixels", async () => {
    const ultra = syntheticGainMap();
    const dual = syntheticGainMap(true);
    await expect(
      inspectJpegGainMapUrl("blob:ultra", { fetch: rangedFetch(ultra) }),
    ).resolves.toEqual({
      kind: "jpeg-gain-map",
      encoding: "ultra-hdr-v1",
      validation: "trailing-jpeg-validated",
    });
    await expect(inspectJpegGainMapUrl("blob:dual", { fetch: rangedFetch(dual) })).resolves.toEqual(
      {
        kind: "jpeg-gain-map",
        encoding: "dual",
        validation: "trailing-jpeg-validated",
      },
    );
  });

  it("returns SDR for a structurally valid JPEG without gain-map metadata", async () => {
    await expect(
      inspectJpegGainMapUrl("blob:sdr", { fetch: rangedFetch(syntheticPlainJpeg()) }),
    ).resolves.toEqual({ kind: "sdr" });
  });

  it("keeps ISO-only signaling unverified", async () => {
    const isoOnly = concat(
      new Uint8Array([0xff, 0xd8]),
      jpegSegment(0xe2, new TextEncoder().encode(`${ISO_21496_NAMESPACE}\0`)),
      new Uint8Array([0xff, 0xda, 0x00, 0xff, 0xd9]),
    );
    await expect(
      inspectJpegGainMapUrl("blob:iso-only", { fetch: rangedFetch(isoOnly) }),
    ).resolves.toEqual({ kind: "unverified", reason: "iso-only-not-validated" });
  });

  it("bounds a non-range 200 response", async () => {
    const fetch = vi.fn(
      async () =>
        new Response(new Uint8Array(20), { status: 200, headers: { "Content-Length": "20" } }),
    );
    await expect(
      inspectJpegGainMapUrl("blob:too-large", { fetch, metadataLimit: 8 }),
    ).resolves.toEqual({ kind: "unverified", reason: "metadata-limit" });
    expect(fetch).toHaveBeenCalledOnce();
  });

  it("classifies the committed JPEG corpus as validated, SDR, and unverified sources", async () => {
    const paths = [
      "test/tstgrp2/g2-PXL_20260822_095305153.jpg",
      "test/tstgrp2/g3-20260822_115745_45.20000_7.20000.jpg",
      "test/tstgrp2/g3-20260822_115835_45.20000_7.20000.jpg",
      "test/tstgrp3/g1-pxl-PXL_20260822_095351447.jpg",
      "test/tstgrp4/g1-PXL_20260822_111856756.jpg",
      "test/tstgrp4/g2-PXL_20260822_111909642.jpg",
      "test/tstgrp4/g3-PXL_20260822_111925636.jpg",
      "test/tstgrp5/g1-PXL_20260822_125342320.jpg",
      "test/tstgrp6/g1-PXL_20260823_150641190.jpg",
      "test/tstgrp7/g2-PXL_20260823_130709050.jpg",
      "test/tstgrp8/g1-PXL_20260823_121248888.jpg",
      "test/tstgrp8/g2-PXL_20260823_123121814.jpg",
      "test/tstgrp9/g1-IMG20260827221839.jpg",
      "test/tstgrp9/g2-IMG20260827221901.jpg",
      "test/tstgrp9/g3-IMG20260827221939.jpg",
      "test/tstgrp9/g1-PXL_20260827_201841598.jpg",
      "test/tstgrp9/g2-PXL_20260827_201902601.jpg",
      "test/tstgrp9/g3-PXL_20260827_201940717.jpg",
    ];
    const results = await Promise.all(
      paths.map(async (path) => {
        const bytes = new Uint8Array(readFileSync(resolve(process.cwd(), "../../", path)));
        return inspectJpegGainMapUrl(`file:${path}`, { fetch: rangedFetch(bytes) });
      }),
    );
    expect(results.filter((result) => result.kind === "jpeg-gain-map")).toHaveLength(13);
    expect(results.filter((result) => result.kind === "sdr")).toHaveLength(2);
    expect(results.filter((result) => result.kind === "unverified")).toHaveLength(3);
    expect(
      results.filter(
        (result) => result.kind === "unverified" && result.reason === "invalid-trailing-jpeg",
      ),
    ).toHaveLength(3);
  });
});

describe("Tauri JPEG gain-map transport", () => {
  it("allows the scoped local asset URL to be inspected through fetch", () => {
    const configuration = JSON.parse(
      readFileSync(resolve(process.cwd(), "../desktop/src-tauri/tauri.conf.json"), "utf8"),
    ) as { app?: { security?: { csp?: string } } };
    const connectSource = configuration.app?.security?.csp
      ?.split(";")
      .find((directive) => directive.trim().startsWith("connect-src "));

    expect(connectSource).toMatch(/(?:^|\s)asset:(?:\s|$)/u);
    expect(connectSource).toContain("http://asset.localhost");
  });
});

describe("JPEG gain-map runtime and presentation policy", () => {
  it("centralizes browser-family policy and keeps unknown/Tauri unverified", () => {
    expect(
      detectJpegGainMapRuntimeCapability({
        runtime: "browser",
        userAgent: "Mozilla/5.0 Firefox/141.0",
      }),
    ).toBe("sdr-only");
    expect(
      detectJpegGainMapRuntimeCapability({
        runtime: "browser",
        userAgent: "Mozilla/5.0 Chrome/140.0 Safari/537.36",
      }),
    ).toBe("native-supported");
    expect(
      detectJpegGainMapRuntimeCapability({ runtime: "browser", userAgent: "WebKit-like bot" }),
    ).toBe("unverified");
    expect(detectJpegGainMapRuntimeCapability({ runtime: "tauri" })).toBe("unverified");
  });

  it("does not infer HDR from the display media query alone", () => {
    const source = {
      kind: "jpeg-gain-map",
      encoding: "ultra-hdr-v1",
      validation: "declared",
    } as const;
    expect(evaluateHdrPresentationStatus(source, "native-supported", "automatic", false)).toBe(
      "sdr-display",
    );
    expect(evaluateHdrPresentationStatus(source, "native-supported", "automatic", true)).toBe(
      "hdr-path-active",
    );
    expect(evaluateHdrPresentationStatus(source, "sdr-only", "automatic", true)).toBe("sdr-codec");
    expect(presentationStatusText("sdr-codec", "sdr-only")).toMatch(/Firefox does not apply/i);
    expect(evaluateHdrPresentationStatus(source, "native-supported", "sdr", true)).toBe(
      "sdr-forced",
    );
    expect(evaluateHdrPresentationStatus(source, "unverified", "automatic", true)).toBe(
      "unverified-runtime",
    );
  });
});
