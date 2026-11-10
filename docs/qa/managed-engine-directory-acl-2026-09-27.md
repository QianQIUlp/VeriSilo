# Windows Managed Engine directory launch difference: ACL causal comparison

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

Date: 2026-09-27. Candidate source: `b6f6b67d3348921fc179aa2225896ef3472e7aca` of `origin/baseline/dev`. Environment: native Windows `10.0.26200.0`. This investigation addresses only the Camoufox/Gecko launch difference in the development worktree. It did not change product code, Engine content, security policy or the install flow.

## Conclusion and boundary

**The confirmed causal variable is the inherited DACL of the browser tree.** With the normal user `telecaster\qiu`, the same absolute path of the browser executable, the same Playwright driver and the same launch options, an isolated copy that inherited the worktree's DACL could not establish a page within 60 seconds. After only the whole-tree DACL of that copy was reset to the inheritance rules of an ordinary parent directory, it established a page in 5.344 seconds. The owner is still `qiu`, and the file content is unchanged. Before the reset, the root directory and `camoufox.exe` had 21 and 14 ACL rules respectively. Afterward they had 5 and 5. The 504 files match the original failing tree item by item by SHA-256.

The failure first appears in the **Gecko sandbox broker spawning the GPU, tab and utility child processes**. The main `camoufox.exe` was already created and the Juggler pipe was already communicating. The logs then record `Failed to launch tab subprocess @SB::LA::SpawnTarget (Error:0)`, three consecutive GPU launch failures and `gBrowser never populated`. This narrows the owning layer to the boundary between filesystem security descriptors and Gecko child-process launch. The logs give no denial code for a specific Windows API and no load failure for a particular DLL. No single ACE, Defender, loader or Chromium sandbox is named as a deeper root cause. Camoufox here reports itself as `Firefox/152.0.4-beta.28`, not Chromium.

The failing tree is owned by `CodexSandboxOnline`. Its inherited DACL contains seven groups of `S-1-15-2-*` package SID rules and additional sandbox user rules. The ordinary successful copy is owned by `qiu` and inherits five ordinary user/SYSTEM/Administrators rules. The owner difference alone does not explain the result: a copy that kept the failing DACL but had owner `qiu` still failed. It is not yet determined which ACE, or whether the root directory or a child file rule, triggers the Gecko child-process failure.

## Controlled results

All of the following minimal probes reuse the Playwright driver of the failing package. They remove `CAMOU_CONFIG_*`/`VERISILO_*`, and use a new temporary Profile, `about:blank`, headful mode and a 60-second timeout. They do not go through Host, the supervisor, the Vault or the product Job. The actual executable is confirmed by the Playwright `<launching>` log. `W` is the original failing tree `C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\integration-performance-completion-37abe4\apps\desktop\src-tauri\target\debug\managed-browser\engine-package\browser`. `U` is the ordinary user directory `C:\Users\qiu\src\VeriSiloEngineProbe`. The full raw logs are in the ignored directory `artifacts/managed-directory/` of this QA worktree.

| Parent process account                 | browser executable                                 | File/directory condition                                                                  | Result                                         |
| -------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Historical probe, account not recorded | `W/camoufox.exe`                                   | Original worktree ACL                                                                     | Failed; Juggler normal, GPU failed three times |
| `CodexSandboxOnline`                   | Short junction inside the worktree pointing to `W` | Same original file objects                                                                | Failed; the path alias itself has no effect    |
| `CodexSandboxOnline`                   | `U/browser/camoufox.exe`                           | 504 hard links to `W`, original file DACL/owner                                           | Failed                                         |
| `CodexSandboxOnline`                   | `U/copy-browser/camoufox.exe`                      | Independent copy, ordinary inherited DACL/owner `qiu`                                     | Failed                                         |
| `qiu`                                  | `W/camoufox.exe`                                   | Original worktree ACL                                                                     | Failed; GPU failed three times                 |
| `qiu`                                  | `U/browser/camoufox.exe`                           | Hard links to `W`, original file DACL/owner                                               | Failed                                         |
| `qiu`                                  | `U/copy-browser/camoufox.exe`                      | Independent copy, ordinary inherited DACL/owner `qiu`                                     | **Passed: 4.894 seconds, 1 page**              |
| `qiu`                                  | `U/dacl-browser/camoufox.exe`                      | Failing-tree DACL preserved through `robocopy /COPY:DATS`, owner `qiu`                    | **Failed: 60-second timeout**                  |
| `qiu`                                  | **The same** `U/dacl-browser/camoufox.exe`         | `icacls /reset /T` run only on this isolated copy; owner, path and file content unchanged | **Passed: 5.344 seconds, 1 page**              |

The independent copy in the ordinary user directory and the DACL comparison copy each have 504 files. Compared item by item by SHA-256 with `W`, the mismatch count is 0 in both cases. The only intentional change between the two `dacl-browser` launches is the DACL of that isolated tree. Each probe uses a new empty Profile path, so it cannot be claimed that the Profile string is byte-for-byte identical. The Profile of the existing successful comparison is also inside the worktree, so a Profile inside the worktree is not by itself a sufficient failure condition. After the probes, the number of leftover related `camoufox.exe` processes was 0. The temporary `U` copies and the junction have been cleaned up. The original Engine tree is still there.

`CodexSandboxOnline` is a separate development-environment boundary. It also failed this round's minimal probe on a clean copy in an ordinary directory and on the installed LocalAppData browser. Those failures are not used to negate the `qiu` DACL comparison, and they do not support inferring that the normal product path fails. The account comes from `whoami /user`. It cannot be inferred from the inherited environment variable `USERNAME=qiu`. The child-process restriction of that sandbox account has not yet been attributed to a specific Windows mechanism.

## Product disposition

For the existing full product evidence, see [Performance and architecture: disposition of the original task](performance-architecture-2026-09-27.md). It used a separate LocalAppData development directory and a new Vault `perf-native-37abe4`. After logs confirmed the real Engine binding, Managed creation, page `readyState=complete`, network, `observed`/`matched`, reobserve, stop/restart and process cleanup all passed. This round changed neither that package nor the product source, so that evidence can still be inherited. This ACL comparison did not prove a defect in the formal install/run path. It is currently scoped as **an inherited-ACL development limit of the current Codex sandbox/worktree**. The product sandbox, Job, Defender, global ACLs and the installer are not modified.

To reproduce the real Managed path during development, use a separate package directory with ordinary inherited ACLs in the normal user context, and register and bind with a new Vault. Treat the Engine package root and browser executable in the driver log as authoritative. Do not judge the binding only by the directory of the desktop exe, and do not change the ACL of the original worktree directly. The comparison above proves the causal role of the DACL. It does not prove that `CodexSandboxOnline` could run the Engine after the original worktree ACL is cleaned.

## Minimal reproduction and evidence not obtained

Dedicated script: [managed-engine-directory-probe.cjs](../../tests/windows/managed-engine-directory-probe.cjs). Command:

```powershell
whoami /user
$env:DEBUG = 'pw:browser'
node tests/windows/managed-engine-directory-probe.cjs <engine-package> <browser-exe> <output-dir>
```

The script prints the actual package root, executable, real path, cwd, Profile, account and TEMP/TMP/LocalAppData, and then the page result or the Playwright error. It does not use a Vault. The independent evidence for the product binding comes from the full product log above. The script passed in the isolated directory after the DACL reset: 6.679 seconds, 1 page. These were not obtained: the GPU child-process exit code, the specific `CreateProcess`/loader failure code, and an item-by-item comparison of inherited handles, Job membership or MSIX identity. No conclusion is drawn about these deeper mechanisms. If a similar failure appears in a normal product directory in the future, native evidence must be collected again in that exact directory and account. This development-limit conclusion must not be applied to it.
