import Ajv2020 from "ajv/dist/2020.js";
import standaloneCode from "ajv/dist/standalone/index.js";
import { compileFromFile } from "json-schema-to-typescript";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const schemaPath = resolve(here, "../../../schema/camcomp.schema.json");
const typesPath = resolve(here, "../src/contracts/camcomp.generated.ts");
const validatorPath = resolve(here, "../src/contracts/camcomp.validator.generated.ts");
const generatedTypes = await compileFromFile(schemaPath, {
  bannerComment: "",
  style: { singleQuote: false, trailingComma: "all", printWidth: 100 },
});
const schema = normalizeNumericFormats(JSON.parse(await readFile(schemaPath, "utf8")));
const ajv = new Ajv2020({
  allErrors: true,
  strict: false,
  strictNumbers: true,
  validateFormats: true,
  code: { esm: true, source: true },
});
const generatedValidator = `// @ts-nocheck\n${standaloneCode(ajv, ajv.compile(schema))}`;

if (process.argv.includes("--check")) {
  const [currentTypes, currentValidator] = await Promise.all([
    readFile(typesPath, "utf8").catch(() => ""),
    readFile(validatorPath, "utf8").catch(() => ""),
  ]);
  if (currentTypes !== generatedTypes || currentValidator !== generatedValidator) {
    console.error("Generated camcomp TypeScript contract is stale. Run npm run contract:generate.");
    process.exitCode = 1;
  }
} else {
  await Promise.all([
    writeFile(typesPath, generatedTypes),
    writeFile(validatorPath, generatedValidator),
  ]);
}

function normalizeNumericFormats(value) {
  if (Array.isArray(value)) return value.map(normalizeNumericFormats);
  if (!value || typeof value !== "object") return value;
  const normalized = Object.fromEntries(
    Object.entries(value).map(([key, child]) => [key, normalizeNumericFormats(child)]),
  );
  const format = normalized.format;
  if (!["double", "uint", "uint16", "uint32", "uint64", "int64"].includes(format)) {
    return normalized;
  }
  delete normalized.format;
  if (format === "double") return normalized;
  normalized.type = "number";
  normalized.multipleOf = 1;
  if (format.startsWith("uint")) normalized.minimum = 0;
  if (format === "uint16") normalized.maximum = 0xffff;
  if (format === "uint32") normalized.maximum = 0xffff_ffff;
  if (format === "uint64") normalized.maximum = Number.MAX_SAFE_INTEGER;
  if (format === "int64") {
    normalized.minimum = Number.MIN_SAFE_INTEGER;
    normalized.maximum = Number.MAX_SAFE_INTEGER;
  }
  return normalized;
}
