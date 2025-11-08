// Modified From https://github.com/zzzgydi/clash-verge/blob/main/scripts/portable.mjs
// GPL-3.0
import fs from "fs-extra";
import path from "path";
import AdmZip from "adm-zip";
import { createRequire } from "module";
import { getOctokit, context } from "@actions/github";

/**
 * 生成 Windows 便携版 ZIP 包
 *
 * - 从 `src-tauri/target/release` 中收集可执行文件 `rgsm.exe`
 * - 将必须的资源文件打包到 `resources/database/database.db`
 * - 创建 `RGSM_<version>_x64-portable.zip`
 * - 如存在 GitHub 发布环境变量，则上传到对应 Release；否则跳过上传
 */
async function resolvePortable() {
  if (process.platform !== "win32") return;

  const releaseDir = "./src-tauri/target/release";
  const bundleDir = path.join(releaseDir, "bundle");

  if (!(await fs.pathExists(releaseDir))) {
    throw new Error("could not found the release dir");
  }

  const zip = new AdmZip();

  zip.addLocalFile(path.join(releaseDir, "rgsm.exe"));
  // 将数据库资源打包进 ZIP（使用项目源中的资源文件）
  // 目标路径：resources/database/database.db
  const dbSrc = path.join("src-tauri", "database", "database.db");
  if (await fs.pathExists(dbSrc)) {
    zip.addLocalFile(dbSrc, "resources/database", "database.db");
  } else {
    console.warn("[WARN]: database resource not found at", dbSrc);
  }

  const require = createRequire(import.meta.url);
  const version = getVersionFromCargo() ?? "unknown";

  const zipFile = `RGSM_${version}_x64-portable.zip`;
  await fs.ensureDir(bundleDir);
  const zipOutPath = path.join(bundleDir, zipFile);
  zip.writeZip(zipOutPath);

  console.log("[INFO]: create portable zip successfully");

  // 清理根目录下旧的便携包（避免重复产物）
  const rootZip = path.join(".", zipFile);
  if (await fs.pathExists(rootZip)) {
    await fs.remove(rootZip);
    console.log("[INFO]: removed old root zip:", rootZip);
  }
  const undefinedZip = path.join(".", "RGSM_undefined_x64-portable.zip");
  if (await fs.pathExists(undefinedZip)) {
    await fs.remove(undefinedZip);
    console.log("[INFO]: removed undefined zip:", undefinedZip);
  }

  // 若无上传凭据，则直接跳过上传，视为本地打包成功
  if (
    process.env.GITHUB_TOKEN === undefined ||
    process.env.RELEASE_ID === undefined
  ) {
    console.log(
      "[INFO]: skip upload, missing GITHUB_TOKEN or RELEASE_ID; local portable build completed"
    );
    return;
  }

  const options = { owner: context.repo.owner, repo: context.repo.repo };
  const github = getOctokit(process.env.GITHUB_TOKEN);

  console.log("[INFO]: upload to ", process.env.RELEASE_ID);

  // https://octokit.github.io/rest.js
  await github.rest.repos.uploadReleaseAsset({
    ...options,
    release_id: process.env.RELEASE_ID,
    name: zipFile,
    data: zip.toBuffer(),
  });
}

/**
 * 从 `src-tauri/Cargo.toml` 读取应用版本号
 *
 * 返回如 `1.5.4`，若解析失败则返回 `null`
 */
function getVersionFromCargo() {
  try {
    const cargoToml = fs.readFileSync(
      path.join("src-tauri", "Cargo.toml"),
      "utf-8"
    );
    const match = cargoToml.match(/\nversion\s*=\s*"([^"]+)"/);
    return match ? match[1] : null;
  } catch (e) {
    console.warn("[WARN]: failed to read version from Cargo.toml", e?.message);
    return null;
  }
}

resolvePortable().catch(console.error);