const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const [packageRoot, executable, outputDir] = process.argv.slice(2);
if (!packageRoot || !executable || !outputDir) {
  console.error('Usage: node managed-engine-directory-probe.cjs <engine-package> <browser-exe> <output-dir>');
  process.exit(2);
}

const browser = path.resolve(executable);
const output = path.resolve(outputDir);
fs.mkdirSync(output, { recursive: true });
const profile = fs.mkdtempSync(path.join(output, 'profile-'));
const { firefox } = require(path.join(path.resolve(packageRoot), 'host/_internal/playwright/driver/package'));
const env = { ...process.env };
for (const key of Object.keys(env)) {
  if (key.startsWith('CAMOU_CONFIG_') || key.startsWith('VERISILO_')) delete env[key];
}

console.log(JSON.stringify({
  phase: 'launch',
  packageRoot: path.resolve(packageRoot),
  executable: browser,
  realExecutable: fs.realpathSync.native(browser),
  cwd: process.cwd(),
  profile,
  parentPid: process.ppid,
  account: execFileSync('whoami', ['/user'], { encoding: 'utf8' }).trim().split(/\r?\n/).at(-1),
  envUsername: process.env.USERNAME,
  temp: process.env.TEMP,
  tmp: process.env.TMP,
  localAppData: process.env.LOCALAPPDATA,
}));

(async () => {
  const start = Date.now();
  const context = await firefox.launchPersistentContext(profile, {
    executablePath: browser,
    headless: false,
    viewport: null,
    timeout: 60000,
    env,
  });
  try {
    const page = context.pages()[0];
    console.log(JSON.stringify({
      phase: 'result',
      launchMs: Date.now() - start,
      pages: context.pages().length,
      url: await page.evaluate(() => location.href),
    }));
  } finally {
    await context.close();
  }
})().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
