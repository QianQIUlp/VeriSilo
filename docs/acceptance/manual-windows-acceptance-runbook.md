# VeriSilo Windows desktop manual acceptance guide (click-only edition)

This guide is only for the person who does the final manual acceptance.

You do not need to open a command line, read source code, build the project, install dependencies, check the registry, or open the browser developer tools. Codex does that work.

You only need to click through the interface in order, observe the results, and record each item as passed, failed or blocked.

## 1. Read this before you start

### 1.1 What Codex should already have prepared for you

Before you start, you should already have:

- A VeriSilo Windows test build that you can double-click to open.

- At least one of Chrome Stable or Edge Stable installed on this computer. The shortest Standard Silo journey needs only one usable browser. In the full two-browser matrix, record a missing browser as "blocked".

- If you are accepting the Companion extension: VeriSilo Companion is already installed in the test browser, and you can see it in the toolbar or the "Extensions" menu.

- If you are accepting a fixed proxy, PAC or Mihomo: Codex has already prepared a dedicated test Silo or given an explicit test configuration. If it is not ready, do not buy, look for or configure a proxy yourself. Record it directly as "blocked".

- If you are accepting install, upgrade and uninstall: Codex has already delivered two installers that you can double-click to run. Without the installers, do not do this section.

If any of the preparation items above is missing, do not fix the environment yourself. Record what is missing, then continue with the other items that are not affected.

### 1.2 Use only this fake data in this run

Do not enter a real account, real password, real proxy credential or private data.

If, after unlocking, you see any real Silo or real business data that must be kept long term, stop this guide immediately and record the current environment as "blocked". Archive, delete, passphrase change and backup restore may be done only in the test Vault that Codex prepared.

Recommended values:

| Purpose                  | Test content              |
| ------------------------ | ------------------------- |
| Initial Vault passphrase | VeriSilo-Manual-2026!     |
| New Vault passphrase     | VeriSilo-Manual-2026-New! |
| Chrome test Silos        | Chrome-A, Chrome-B        |
| Edge test Silos          | Edge-A, Edge-B            |
| Deletable Silo           | Delete-Me                 |
| Bookmark name in A       | VS-A-ONLY                 |
| Bookmark name in B       | VS-B-ONLY                 |
| Backup file name         | verisilo-manual.backup    |

If Codex has already initialized the current test Vault, use the test passphrase Codex delivered. Do not recreate or delete the existing data.

### 1.3 How to record results

After each small section, record only one result:

- **Passed**: What you see matches "Normal", and none of the cases under "Abnormal" appear.

- **Failed**: The feature can be operated, but the result is wrong, the interface freezes, data leaks into another Silo, or a safety red line appears.

- **Blocked**: An external condition is missing, such as the browser, extension, installer, test proxy or network, so you cannot start.

When you find an abnormality, take one screenshot that includes the whole application window, and note the section and the button you just clicked. Do not clear caches, delete directories or change system settings yourself.

## 2. Shortest acceptance route

If you only want to first judge whether the current version is worth further testing, do these six things in order:

1. Double-click to open VeriSilo, and confirm that only one desktop window appears.

2. Create or unlock the Vault, then click **立即锁定** ("Lock now"), and confirm that a wrong passphrase cannot unlock and the correct passphrase can.

3. Create two Silos that use the same browser and whose network mode is **直连** ("Direct").

4. In A, save the bookmark **VS-A-ONLY**, and in B confirm you cannot see it. Then go back to A and confirm A's bookmark is still there.

5. Close the Silo browser, and confirm that the desktop finally goes from **运行中** ("Running") back to **空闲** ("Idle").

6. Edit, archive and restore a Silo, and confirm that its data and bookmarks are not lost.

Only when all six items pass do you continue with the rest of the full acceptance.

## 3. Startup and single-window check

### 3.1 First launch

1. In File Explorer, find the VeriSilo test build that Codex prepared.

2. Double-click **verisilo.exe** and wait at most 30 seconds.

3. Look at the top of the window and the page body.

**Normal:**

- Only one VeriSilo window appears.

- You can see **仅本机** ("Local only") at the top of the window.

- On first use you see **创建保险库** ("Create Vault"). If a Vault already exists, you see **解锁保险库** ("Unlock Vault").

- The page text is clear, with no white screen, black screen, layout so misplaced that it cannot be clicked, or constant flicker.

**Abnormal:**

- No window appears after the double-click, or the window exits immediately after appearing.

- After more than 30 seconds the page is still blank or keeps loading.

- The Chrome or Edge Profile you use every day is opened at startup at the same time.

- Windows shows repeated crash prompts.

### 3.2 Opening it again should only activate the existing window

1. Keep the VeriSilo window open.

2. Go back to File Explorer and double-click the same **verisilo.exe** once more.

3. Look at the taskbar and the desktop.

**Normal:** The original VeriSilo window is brought to the front, and no second independently operable desktop window appears.

**Abnormal:** Two independent VeriSilo windows appear, both windows can each start a Silo, or the first window freezes after the second launch.

## 4. Vault

### 4.1 Create the Vault for the first time

If the page shows **创建保险库** ("Create Vault"):

1. In **保险库口令** ("Vault passphrase"), enter the test passphrase **VeriSilo-Manual-2026!**.

2. Click **创建本地保险库** ("Create local Vault").

3. Wait until you reach **环境概览** ("Environment overview").

**Normal:**

- The top shows **保险库已解锁** ("Vault unlocked").

- The page shows **环境概览** ("Environment overview"), **创建 Silo** ("Create Silo"), **保险库与数据** ("Vault and data") and **运行位置设置** ("Run location settings").

- The status card shows that the local Vault is unlocked and the browser process is idle.

**Abnormal:**

- With fewer than 12 characters, the button still allows creation.

- After creation it returns to the first-time creation page, or the page shows no error but never enters the overview.

- The passphrase stays displayed on the page in plain text for a long time.

If the page shows **解锁保险库** ("Unlock Vault"), continue directly with the passphrase of the current test Vault.

### 4.2 Lock, wrong passphrase and correct passphrase

1. In **环境概览** ("Environment overview"), click **立即锁定** ("Lock now").

2. Look at the top status and the page content.

3. Enter a passphrase that is certainly wrong, and click **解锁** ("Unlock").

4. Then enter the correct passphrase and click **解锁** ("Unlock").

**Normal:**

- After locking, the top shows **保险库已锁定** ("Vault locked").

- Sensitive content such as the Silo list, network results and report selection can no longer be operated.

- The wrong passphrase clearly fails and the Vault stays locked.

- The correct passphrase enters the overview successfully, and the original Silo list is restored.

**Abnormal:**

- The wrong passphrase can unlock.

- After locking, you can still start, edit, archive or delete a Silo.

- After unlocking, the original Silos disappear for no reason.

### 4.3 Change the passphrase

After this step, use the new passphrase from here on.

1. Open **保险库与数据** ("Vault and data").

2. Find **更换保护本地配置的口令** ("Change the passphrase that protects local configuration").

3. In **当前口令** ("Current passphrase"), first enter a wrong passphrase.

4. In **新口令** ("New passphrase") and **再输一次** ("Re-enter"), enter **VeriSilo-Manual-2026-New!**.

5. Click **更新保险库口令** ("Update Vault passphrase").

6. After confirming the failure, enter the correct current passphrase instead and click **更新保险库口令** ("Update Vault passphrase") again.

7. Go back to **环境概览** ("Environment overview") and click **立即锁定** ("Lock now").

8. First unlock with the old passphrase, then unlock with the new passphrase.

**Normal:**

- When the current passphrase is wrong, nothing is updated and the page gives a clear error.

- When the current passphrase is correct and the two new passphrases match, the update succeeds.

- After the update, the old passphrase fails and the new passphrase succeeds.

- The Silo list and data are not lost because of the passphrase change.

**Abnormal:** The old passphrase can still unlock, a wrong current passphrase can still change the passphrase, the two new passphrases do not match but can still be submitted, or data is lost after the change.

### 4.4 15-minute auto-lock

You can do this item last.

1. After unlocking the Vault, do not operate VeriSilo for 15 minutes.

2. Go back to the window and try to click any page that needs the Vault.

**Normal:** The page requires you to unlock again, and the expired session cannot continue to be used.

**Abnormal:** After more than 15 minutes you can still start, edit or delete a Silo directly.

## 5. Create a Direct Silo

First, use the browser that the page discovers automatically to create A and B of the same family. The steps below use Chrome-A/Chrome-B as the example. If this computer has only Edge installed, replace Chrome with Edge in the names and steps. The other browser family belongs to the full matrix, and its absence does not block the shortest Standard Silo journey.

### 5.1 Create the first Silo with the recommended defaults

1. Click **创建 Silo** ("Create Silo") at the top.

2. Confirm that the page shows **推荐设置已就绪** ("Recommended settings ready") and lists the automatically discovered Google Chrome or Microsoft Edge, **Windows 本机** ("Windows (this machine)"), **直连** ("Direct") and **独立 Profile** ("Separate Profile").

3. In **Silo 名称** ("Silo name"), enter **Chrome-A** or **Edge-A**, matching the automatically discovered browser, and then choose an easily recognizable color.

4. Do not expand **高级设置** ("Advanced settings"). If the page does not discover a browser automatically, or by default shows Linux, a proxy, PAC or Mihomo, record "Failed". Do not look for the browser path by hand to cover up a default-path problem.

5. Read the three items **Profile 隔离** ("Profile isolation"), **设备与浏览器指纹** ("Device and browser fingerprint") and **受控指纹** ("Controlled fingerprint"). They should show `native`, `inherit` and `unavailable` respectively, and explain in plain language that the login state is separate, the fingerprint follows this machine, and Standard Silo provides no controlled fingerprint.

6. Check **我已核对以上边界** ("I have reviewed the boundaries above") and click **创建 Silo** ("Create Silo").

**Normal:** It returns to the overview automatically, and a card with the matching name appears. The browser matches the automatic discovery result, the network shows Direct, and the honest boundary `native / inherit / unavailable` continues to be shown.

**Abnormal:** Advanced settings are expanded by default, you are asked to type the browser path by hand, Linux/proxy is selected by default, the browser family changes after creation, or a separate Profile is written as fingerprint isolation/verified.

### 5.2 Create the second Silo of the same family

Repeat the recommended default path exactly, and create **Chrome-B** or **Edge-B** with a different color. The two Silos must be bound to different separate Profiles.

### 5.3 Optional: switch to the other installed browser

Only the full two-browser matrix needs this item. Expand **高级设置** ("Advanced settings"), choose the other browser among the detected browsers, and then create the matching A/B. A manual browser path, the Linux run location and a non-Direct network should also appear only here. Expanding and collapsing must not lose the settings already filled in.

If the other browser is not installed, record this subsection as "blocked", but continue with the shortest Standard Silo journey.

## 6. Start, close and running state

### 6.1 Start one Silo

1. In **环境概览** ("Environment overview"), find **Chrome-A**.

2. Click **启动 Silo** ("Start Silo").

3. Wait for the test browser window to appear.

4. Go back to VeriSilo and look at the Chrome-A card and the top status.

**Normal:**

- A new Chrome test window opens.

- The Chrome-A card shows **运行中** ("Running").

- The card clearly says that closing this Silo's browser window stops it, and there is no button that force-closes your everyday Chrome/Edge.

- The **启动 Silo** ("Start Silo") buttons of the other Silos are temporarily not clickable.

- The new window should not automatically carry over the signed-in accounts, bookmarks, history or extension state of your everyday Chrome.

**Abnormal:**

- The window that opens is your everyday Chrome window, and you can see real accounts or real bookmarks.

- One click opens several test windows.

- While Chrome-A is running, Chrome-B or Edge-A can still be started.

- The browser is already open, but the desktop still shows idle for a long time.

If you see the content of the everyday Profile, stop this section immediately and record "Failed". Do not keep entering anything in that window.

### 6.2 Normal close

1. Close all the test browser windows that Chrome-A just opened.

2. Wait a few seconds. Do not click any stop or process-cleanup button.

**Normal:** Chrome-A no longer shows running, the "Browser process" at the top (**浏览器进程**, "Browser process") returns to **空闲** ("Idle"), and the start buttons of the other Silos become available again.

**Abnormal:** All the test browsers are closed but it still shows running for a long time; you have to click a recheck by hand to return to idle; or, to recover the state, the application asks you to close your everyday Chrome.

### 6.3 Start all four Silos once

In the order "start one — observe — close all test windows — recheck", start Chrome-B, Edge-A and Edge-B in turn.

Do not start two Silos at the same time, and do not close the browser you use every day for the sake of the test.

## 7. The most important A/B isolation check

This step checks only results that an ordinary user can see. The underlying storage, lock files and default-Profile integrity are the job of the automated tests.

### 7.1 Write a dedicated bookmark in Chrome-A

1. Start **Chrome-A**.

2. Enter **https://example.com** in the address bar and open it.

3. Click the bookmark star on the right of the address bar.

4. Change the name to **VS-A-ONLY** and save.

5. Then open the three-dot menu at the top right of the browser, go to **历史记录** ("History"), and confirm that the example.com you just visited is visible.

6. Close all Chrome-A windows, and confirm in VeriSilo that the state returns to idle.

**Normal:** The bookmark saves successfully, and this visit can be seen in the history.

**Abnormal:** The bookmark cannot be saved, disappears right after saving, or appears in your everyday Chrome.

### 7.2 Chrome-B must not see A's content

1. Start **Chrome-B**.

2. Open the three-dot menu at the top right, and then open **书签和清单** ("Bookmarks and lists") or **收藏夹** ("Favorites").

3. Look for **VS-A-ONLY**.

4. Open the history and look for the example.com visit made earlier in A.

5. After confirming you cannot see either, open https://example.com and save a bookmark named **VS-B-ONLY**.

6. Close all Chrome-B windows, and confirm VeriSilo returns to idle.

**Normal:** B cannot see A's bookmark or the history A just made. B can save its own **VS-B-ONLY**.

**Abnormal:** **VS-A-ONLY** appears in B, or browsing records that were made only in A appear. Either one is a severe isolation failure.

### 7.3 Go back to Chrome-A and check persistence

1. Start **Chrome-A** again.

2. Open the bookmark list.

3. Confirm that **VS-A-ONLY** is still there, and confirm that **VS-B-ONLY** is not.

4. Close all Chrome-A windows.

**Normal:** A's own data is kept across restarts, and B's data has not been mixed in.

**Abnormal:** A's bookmark is lost, or B's bookmark appears in A.

### 7.4 Repeat the same checks in Edge

In **Edge-A**, save **VS-A-ONLY**. In **Edge-B**, confirm you cannot see it and save **VS-B-ONLY**. Finally go back to Edge-A and check again.

In Edge, the bookmark button is usually called **收藏夹** ("Favorites"), and it is in the star on the right of the address bar or in the three-dot menu at the top right.

**Normal:** Edge-A and Edge-B cannot see each other's bookmarks or history, and each keeps its own data after a restart.

**Abnormal:** Either side sees content that is dedicated to the other side, or test data enters your everyday Edge Profile.

## 8. Edit, archive, restore and permanently delete

### 8.1 Edit the name and color

1. Confirm that no Silo is running.

2. On the Chrome-A card, click **编辑** ("Edit").

3. Change the name to **Chrome-A-Renamed**, and pick another color.

4. Do not check "替换网络配置" ("Replace network configuration") or "替换引擎配置" ("Replace engine configuration").

5. Click **保存更改** ("Save changes").

6. Start the renamed Silo, confirm that **VS-A-ONLY** is still there, and then close the browser.

**Normal:** The name and color are updated, the original bookmark is still there, and the network is still Direct.

**Abnormal:** An ordinary rename loses the bookmark, changes the browser, replaces the network, or creates a second copy of the Silo.

### 8.2 Archive and restore

1. On the renamed Silo's card, click **归档** ("Archive").

2. Scroll down to the **已归档** ("Archived") area.

3. Confirm that this Silo appears here and has disappeared from the startable list.

4. Click **恢复** ("Restore").

5. Confirm that it appears in the startable list again.

6. Start it once more, confirm that **VS-A-ONLY** is still there, and then close the browser.

**Normal:** Archiving only hides the start entry and does not delete browser data. After restoring, the original data and bookmark are still there.

**Abnormal:** Archiving deletes the data directly, a restored Silo becomes a blank Profile, or an archived item can still be started directly.

### 8.3 Dangerous operations must be refused while running

1. Start **Chrome-A-Renamed**.

2. Go back to VeriSilo and look at **编辑** ("Edit") and **归档** ("Archive") on that card.

3. Look at **启动 Silo** ("Start Silo") of the other Silos.

**Normal:** A running Silo cannot be edited or archived, and the other Silos cannot start at the same time.

**Abnormal:** A running Silo can still be edited, archived or permanently deleted, or a second Silo can be started.

After the check, close the test browser and confirm that the state returns to idle.

### 8.4 Test permanent delete only on a disposable Silo

1. Create a Direct Silo named **Delete-Me**.

2. Do not put any data in it that needs to be kept.

3. Go back to the overview and click **归档** ("Archive").

4. In the **已归档** ("Archived") area, click **永久删除** ("Delete permanently").

5. The first time, click **取消** ("Cancel") in the confirmation box, and confirm that Delete-Me is still there.

6. Click **永久删除** ("Delete permanently") again, read the prompt carefully, and click **确定** ("OK").

**Normal:** Cancel deletes nothing. After confirming, only Delete-Me disappears, and the other Silos are not affected.

**Abnormal:** It deletes without confirmation, deletes anyway after cancel, deletes the wrong Silo, or affects your everyday Chrome/Edge.

## 9. Desktop network check

### 9.1 User-initiated check

1. Go back to **环境概览** ("Environment overview").

2. Find **检查桌面端实际看到的网络出口** ("Check the network exit the desktop actually sees").

3. Click **同意并检查** ("Agree and check").

4. Wait for the result to appear.

**Normal:**

- When the network is available, the page shows the public IP, network ownership, exit time zone, public DNS and DNSSEC status.

- The page clearly says this is only the exit that the desktop controller sees, and does not pass itself off as the exit of any Silo browser.

- After you click **清除** ("Clear"), the result disappears from the current interface.

**Abnormal:**

- A request is sent to the detection service automatically without you clicking agree.

- It keeps showing that it is checking and cannot recover.

- When the network fails, it still shows an old result and claims it is this run's successful result.

If the computer is currently offline, record this item as "blocked". Do not change the system network settings.

### 9.2 Direct Silo

1. Start any Silo whose network is **直连** ("Direct").

2. Open https://example.com.

**Normal:** When the network is currently available, the page opens normally. The desktop card still shows Direct and has not become a proxy for no reason.

**Abnormal:** You chose Direct at creation, but at runtime it shows that an unknown proxy is in use. Or after a failed start, the desktop still claims it is running normally.

### 9.3 Fixed proxy and fail-closed

Do this section only when Codex has already prepared test Silos named **Proxy-Working** and **Proxy-Broken**. Without these two cards, record "blocked".

1. Start **Proxy-Working**.

2. Confirm that the browser can open the test web page that Codex specified, and then close the browser.

3. Start **Proxy-Broken**.

4. Observe the desktop state and whether the browser appears.

**Normal:** The working proxy can start. A configuration that is deliberately unusable and marked "必须代理" ("required proxy") refuses to start, or clearly shows **网络已阻断** ("Network blocked"), and never quietly switches to Direct.

**Abnormal:** Proxy-Broken can still visit web pages like an ordinary Direct connection, or after a failure the page claims the proxy has been verified.

PAC, Mihomo, remote environments and virtual environments follow the same principle. Accept them only when Codex has already prepared a clickable test card, and do not set up external services yourself.

## 10. Encrypted backup and restore

This step backs up only the Silo configuration. It does not back up bookmarks, Cookies, history or the whole browser Profile. The interface must state this clearly.

### 10.1 Prepare an empty folder with File Explorer

1. Open Windows **文件资源管理器** ("File Explorer").

2. Open **文档** ("Documents").

3. Click **新建** ("New") and choose **文件夹** ("Folder").

4. Name the folder **VeriSilo 人工验收** ("VeriSilo manual acceptance").

5. Open that folder, click the address bar and copy the displayed location.

### 10.2 Create the backup

1. Go back to VeriSilo and open **保险库与数据** ("Vault and data").

2. Find **备份 Silo 配置，不冒充浏览器数据备份** ("Back up Silo configuration, not a browser data backup").

3. In **目标文件绝对路径** ("Absolute path of target file"), paste the location you just copied, and add the file name **verisilo-manual.backup** at the end.

4. Click **创建加密备份** ("Create encrypted backup").

5. Go back to File Explorer and confirm that the file appears.

6. Go back to VeriSilo and click **创建加密备份** ("Create encrypted backup") again for the same target.

**Normal:** The first time shows success and the file exists. The second time refuses to overwrite the existing file and does not replace it silently.

**Abnormal:** A success prompt appears but the file does not exist, the file size is always zero, or the second time overwrites directly without a clear refusal.

### 10.3 A wrong passphrase cannot restore

1. In the **恢复保险库** ("Restore Vault") area, enter the same backup file location in **备份文件绝对路径** ("Absolute path of backup file").

2. In **该备份的口令** ("Passphrase of this backup"), enter a wrong passphrase.

3. Check **我确认覆盖当前保险库记录，并理解 Profile 文件不在此备份中** ("I confirm overwriting the current Vault records and understand that Profile files are not in this backup").

4. Click **验证并恢复保险库** ("Verify and restore Vault").

**Normal:** The restore is refused, and the current Silo list, names and running state do not change.

**Abnormal:** A wrong passphrase can also restore, or after the failure the current Vault content is emptied, renamed or locked up.

### 10.4 No restore while a Silo is running

1. Start any test Silo.

2. Go back to **保险库与数据** ("Vault and data"), and enter the correct backup file location and the passphrase used when the backup was created.

3. Check the overwrite confirmation and click **验证并恢复保险库** ("Verify and restore Vault").

**Normal:** The application clearly refuses before modifying the Vault, and the running Silo and the current data do not change.

**Abnormal:** The Vault is replaced while the browser is still running, or after the restore the desktop mistakes the old browser for the new running state.

When finished, close the test browser and return to the idle state.

### 10.5 Correct restore

1. Confirm that no Silo is running.

2. Enter the correct backup file location and the passphrase used when the backup was created.

3. Check the overwrite confirmation.

4. Click **验证并恢复保险库** ("Verify and restore Vault").

5. Wait for the restored state to reload.

**Normal:** The restore succeeds. The Silo metadata already in the backup reappears. The page does not keep sensitive input that was not committed before the restore. The interface continues to state clearly that Profile files are not in the backup.

**Abnormal:** A temporary form from before the restore is mixed in after the restore, it claims the browser Profile was also backed up, or it stays for a long time at **恢复后的状态仍处于隔离** ("The restored state is still isolated") and cannot reload.

## 11. Local report export

### 11.1 Conditions that enable the buttons

1. In **环境概览** ("Environment overview"), find **只导出你明确选中的一个 Silo** ("Export only the one Silo you explicitly select").

2. First do not select a Silo, and look at **下载 JSON** ("Download JSON") and **下载 HTML** ("Download HTML").

3. Select a test Silo, but do not check the confirmation box yet.

4. Then check **我确认只导出所选 Silo 的脱敏本地报告** ("I confirm exporting only the redacted local report of the selected Silo").

**Normal:** When nothing is selected, both buttons cannot be clicked. When only a selection is made without confirmation, they still cannot be clicked. Only after the selection and the confirmation can they be clicked.

**Abnormal:** The application selects a Silo for you by default, or allows a download without confirmation.

### 11.2 Download and check by eye

1. Click **下载 HTML** ("Download HTML").

2. Open Windows **文件资源管理器** ("File Explorer"), go to **下载** ("Downloads"), find the HTML file just generated, and double-click to open it.

3. Confirm that the page can be read offline, and that the title, Silo name and boundary statement are clear.

4. Go back to VeriSilo, click **下载 JSON** ("Download JSON"), and then go to the **下载** ("Downloads") folder to confirm that the file really appears. You are not required to open or read the technical content.

**Normal:**

- Both kinds of files really appear in the downloads list.

- The HTML is a readable static report, not a blank page or garbled text.

- By eye, you cannot see the Vault passphrase, a proxy password, a full local file location, or any obvious complete secret value.

**Abnormal:**

- No file exists after the click, but a success prompt appears.

- After the HTML is opened, it keeps loading external web pages, performs a redirect, or shows a script error.

- The report directly shows the Vault passphrase, proxy credentials or the full user directory location.

Deeper checks of redaction, escaping and format are the job of the automated tests. You are not required to read the JSON.

## 12. Companion extension basic functions

Do this chapter only when **VeriSilo Companion** can already be seen in the test Silo browser. If you cannot see it, record "blocked". Do not open the extension management page yourself to install or debug it.

### 12.1 Open the side panel

1. Start a test Silo.

2. Open https://example.com.

3. Click the VeriSilo icon on the browser toolbar. If the icon is not pinned, first click the puzzle-shaped **扩展** ("Extensions") button, and then click **VeriSilo**.

**Normal:** The VeriSilo side panel opens on the right. At the top you can see **身份概览** ("Identity overview"), **轻量隔离** ("Light isolation"), **实验室** ("Labs") and **技术数据** ("Technical data"). Only one page is shown at a time.

**Abnormal:** A blank page opens, the side panel flashes by, the four pages are stacked together at the same time, or the browser crashes.

### 12.2 Site permission and scan

1. In **身份概览** ("Identity overview"), click **扫描当前页面** ("Scan current page").

2. If the browser asks whether to allow access to the current site, click deny the first time.

3. Observe the side panel, and then expand **扫描提示“无法访问”？** ("Scan says 'cannot access'?").

4. Click **请求当前站点权限** ("Request current-site permission"), and allow it this time.

5. Click **扫描当前页面** ("Scan current page") again.

**Normal:** On deny, it clearly says the site is not authorized and does not show a forged success result. After allowing, a new report for the current page appears and the waiting state ends.

**Abnormal:** It still shows success after the permission is denied; it keeps spinning after allowing; scanning example.com shows the report of another site from earlier.

If the first scan shows no permission prompt and succeeds directly, the permission was already prepared by the test environment. You can continue, and there is no need to reset it on purpose.

### 12.3 The report must follow the tab

1. Keep the example.com tab that has been scanned.

2. Open a new tab and go to https://example.org.

3. In the new tab, open the VeriSilo side panel, but do not scan.

4. Switch back to the example.com tab.

**Normal:** example.org shows **还没有扫描结果** ("No scan result yet"). After switching back to example.com, the original report reappears.

**Abnormal:** The new tab directly shows the old tab's report, or report ownership becomes confused after switching back.

### 12.4 Report history and readable export

1. On the scanned tab, click **技术数据** ("Technical data").

2. Click **导出可读 HTML** ("Export readable HTML").

3. Click OK in the confirmation box.

4. Open the exported HTML from the browser's download list.

5. Go back to the side panel and click **清除本地报告历史** ("Clear local report history").

6. The first time, click cancel in the confirmation box, and confirm that the history is still there.

7. Click it again and confirm, and confirm that the history is cleared.

**Normal:** The exported file is readable. Canceling the clear keeps the history. After confirming, the history is cleared. The current page does not suddenly show another site's report because the history was cleared.

**Abnormal:** Cancel also deletes, the download is empty, the report is garbled, or the export directly contains an obvious complete high-sensitivity value.

### 12.5 Network check

1. Go back to **身份概览** ("Identity overview") and find the network exit card.

2. Click **同意并检查当前环境出口** ("Agree and check the current environment's network exit").

3. If the browser asks whether to allow access to the detection service, click allow.

4. Wait for the result.

5. After the check completes, click **清除结果并撤销权限** ("Clear result and revoke permission"), and confirm that the result disappears from the side panel.

**Normal:** It shows the IP, region, network ownership and DNS result. When the desktop integration is not ready, it clearly shows **仅本地** ("Local only") and does not pretend to be bound to a Silo. After clearing, the result disappears.

**Abnormal:** It goes online without the user's consent; on failure it reuses an old result; the desktop Silo has stopped but it is still shown as bound.

## 13. Light isolation

### 13.1 Enable and restore the recommended protection

1. In the side panel, click **轻量隔离** ("Light isolation").

2. Find **减少网络旁路信号** ("Reduce network side-channel signals").

3. Click **启用推荐保护** ("Enable recommended protection").

4. If the browser asks for the privacy-control permission, click allow.

5. Observe the state of WebRTC and network prediction.

6. Click **恢复原设置** ("Restore original settings").

**Normal:** After enabling, it shows applied, or clearly says it is controlled by browser policy. After restoring, it shows restored or that there is no setting to restore, and the button leaves the busy state.

**Abnormal:** It still claims applied after the permission is denied; it hangs forever after you click restore; the restore fails but shows success.

### 13.2 Open a privacy window

1. Keep an ordinary web page open.

2. Click **用当前网站打开隐私窗口** ("Open a privacy window with the current site").

**Normal:** Chrome opens an incognito window or Edge opens an InPrivate window, and opens the current site.

**Abnormal:** An ordinary window opens, the everyday browser Profile opens, several windows open in a row, or the side panel fails without any prompt.

If Codex has also prepared an isolation test page with "保存" ("Save") and "读取" ("Read") buttons, check by the page text that the data in an ordinary window and a privacy window cannot be seen by each other. Without this test page, record "privacy-window data isolation" as "blocked", and do not build a page yourself.

## 14. Desktop and Companion integration

This chapter requires that the desktop is already open, the Vault is unlocked, a test Silo is running, and Companion is already installed in that Silo. If any condition is missing, record "blocked".

### 14.1 Activate the desktop from the extension

1. Open Companion in the Silo browser.

2. Click **轻量隔离** ("Light isolation").

3. Click **打开 VeriSilo 桌面端** ("Open VeriSilo desktop").

**Normal:** The already-open VeriSilo window is brought to the front, and no second desktop controller appears.

**Abnormal:** It says the desktop cannot be found, a second desktop window opens as a duplicate, or the original desktop stops responding.

### 14.2 The network result belongs to the current Silo

1. Go back to the running Silo browser.

2. In **身份概览** ("Identity overview") of Companion, click **同意并检查当前环境出口** ("Agree and check the current environment's network exit").

3. Go back to the desktop, find **Silo 内证据** ("Evidence inside Silo") in **环境概览** ("Environment overview"), and look at the current Silo's records.

**Normal:** The extension clearly shows **桌面 Silo 已接收** ("Desktop Silo received"). The desktop shows a new record for the same Silo at the matching time.

**Abnormal:** The result is recorded under another Silo or under an old run that has already stopped, or the extension shows submitted but the desktop has no matching record at all.

### 14.3 After a Silo stops, the old binding cannot be reused

1. Close all browser windows of the current Silo.

2. Go back to the desktop, and if needed click **复查运行与网络** ("Recheck run and network") until the state is idle.

3. Then open Companion from another prepared test browser environment and run a network check.

**Normal:** The result is clearly degraded to **仅本地** ("Local only") or shows that the desktop Silo is not ready. It never keeps showing a binding to the Silo that just stopped.

**Abnormal:** After the stop, it still shows for a long time that the old Silo is receiving results, or it records a new request under the old run.

## 15. Labs manual check

Labs may make the current web page briefly abnormal. Operate only on test pages like example.com that have no login and no form content.

### 15.1 The capability boundary must be shown truthfully

1. In Companion, click **实验室** ("Labs").

2. Look at the three capability cards.

**Normal:**

- "网页后台任务一致性" ("Web background task consistency") shows **有限覆盖** ("Limited coverage"), and is off by default.

- "Cookie 仓库虚拟化" ("Cookie store virtualization") shows **当前扩展不支持** ("Not supported by the current extension").

- "Set-Cookie 全面截获" ("Full Set-Cookie interception") shows **当前扩展不支持** ("Not supported by the current extension").

**Abnormal:** Limited coverage is written as full protection, or the two unsupported capabilities are shown as enableable.

### 15.2 Cancel, enable and manual stop

1. Click **为当前站点开启 2 分钟** ("Enable for the current site for 2 minutes").

2. The first time, click cancel in the confirmation box.

3. Confirm that the state is still off by default.

4. Click **为当前站点开启 2 分钟** ("Enable for the current site for 2 minutes") again, and confirm this time.

5. If a site permission prompt appears, click allow.

6. Wait for the state to become running.

7. Click **立即恢复并停用** ("Restore now and disable").

**Normal:** Cancel does not enable it. After confirming, it shows limited coverage and the stop button is available. After a manual stop, it shows restored, and a new, understandable, redacted local receipt is added.

**Abnormal:** Cancel also enables it, the state stays at enabling forever, it still shows running after the stop, or the receipt claims restored but the interface still cannot be operated again.

### 15.3 Automatic stop at expiry

1. Enable the 2-minute experiment again.

2. Do not operate the web page. Wait 2 minutes 10 seconds.

3. Look at the state and the latest receipt.

**Normal:** The experiment stops automatically, the state is no longer running, and the latest receipt says it stopped because of expiry or timeout and completed the restore.

**Abnormal:** It is still running after the time has passed, or after the stop there is no explainable result.

### 15.4 Automatic stop on navigation or permission revocation

1. Enable the experiment again on example.com.

2. In the same tab, open https://example.org.

3. Reopen the side panel and look at the Labs state.

4. Go back to example.com and enable the experiment again.

5. Switch to **身份概览** ("Identity overview"), expand the permission help, and click **撤销当前站点长期权限** ("Revoke long-term permission for the current site").

**Normal:** The experiment stops automatically after navigation, and also stops automatically after the permission is revoked. Both times there is a clear stop reason and restore result.

**Abnormal:** It still shows running after you leave the site or revoke the permission, or it keeps allowing the old site to use the experiment state.

### 15.5 Clear receipts

1. Click **清除收据** ("Clear receipts").

2. The first time, click cancel in the confirmation box.

3. Click it again and confirm.

**Normal:** After cancel the receipts are kept, and after confirming they are cleared.

**Abnormal:** Cancel also clears them, or old receipts keep reappearing after confirming.

## 16. Install, upgrade and uninstall (only if you have the installers)

Without the old-version and new-version installers prepared by Codex, record the whole chapter as "blocked". Do not pass off the current debug build as an installer.

### 16.1 Install the old version

1. Double-click the old-version installer.

2. Complete the installation with the default options in the installation wizard.

3. Open VeriSilo from the Windows Start menu.

4. Create a test Vault and a Direct Silo named **Upgrade-Keep**.

5. Close the application normally.

**Normal:** A standard user can complete the installation. The Start menu has an entry. The application starts normally and can save the test Silo.

**Abnormal:** It requires closing your everyday Chrome/Edge before it can install, no entry can be found after installation, or the first launch shows a white screen.

### 16.2 Upgrade to the new version

1. Double-click the new-version installer.

2. Complete the upgrade with the wizard.

3. Open VeriSilo from the Start menu.

4. Unlock the Vault and look for **Upgrade-Keep**.

**Normal:** The upgrade keeps only one application entry. The original Vault can be unlocked. Upgrade-Keep is still there.

**Abnormal:** Two versions appear at the same time, the original data is lost, the old passphrase stops working for no reason, or the upgrade requires you to delete the old version by hand.

### 16.3 Uninstall

This step removes the application itself. Run it only when you are sure the other manual tests of this round are finished.

1. Open Windows **设置** ("Settings").

2. Click **应用** ("Apps"), then go to **已安装的应用** ("Installed apps").

3. Find VeriSilo and click **卸载** ("Uninstall") in the menu on the right.

4. Follow the prompts to complete the uninstall.

**Normal:** The application entry is removed. The uninstall interface clearly says whether user data is kept. The everyday Chrome/Edge Profile is not deleted or damaged.

**Abnormal:** The uninstall deletes everyday browser data by mistake, two application entries are left behind, or it deletes user data without any prompt.

## 17. If you see any of these, judge it failed immediately

None of the following is a "small problem", and you do not need to keep trying repeatedly:

- A test Silo opened the everyday Chrome/Edge Profile, or everyday bookmarks, accounts or history appear in the test window.

- A can see bookmarks, history or site state saved only in B, or the reverse.

- A wrong Vault passphrase can unlock, change the passphrase or restore a backup.

- A running Silo can be edited, archived or permanently deleted.

- A test configuration that is "必须代理" ("required proxy") quietly switches to Direct after a failure.

- The browser is already closed, but the desktop keeps treating the old run as the current run for a long time. Or a new result is recorded under an old Silo.

- After opening VeriSilo repeatedly, two desktop windows appear that can each control a Silo.

- A report directly shows the Vault passphrase, a proxy password, the full local user directory or another obvious secret.

- Any button has no result, no error and cannot be canceled for more than 30 seconds, and reopening the page still does not recover it.

When this happens: capture the whole window, note the section number, Silo name, browser and the button you just clicked, and then close the current test window. Do not delete any file. Let Codex continue from the environment as it was left.

## 18. Final checklist

| Acceptance item           | Pass criterion                                                                         | Your result |
| ------------------------- | -------------------------------------------------------------------------------------- | ----------- |
| Desktop startup           | An operable window appears within 30 seconds                                           |             |
| Single window             | Opening again only activates the existing window                                       |             |
| Vault                     | Wrong passphrase fails, correct passphrase succeeds                                    |             |
| Change passphrase         | Old passphrase stops working, new passphrase takes effect                              |             |
| Chrome Silo               | A/B can both be created, started and closed                                            |             |
| Edge Silo                 | A/B can both be created, started and closed                                            |             |
| Chrome isolation          | A/B bookmarks and history are not visible to each other                                |             |
| Edge isolation            | A/B bookmarks and history are not visible to each other                                |             |
| Default Profile           | No everyday browser content was brought in or changed                                  |             |
| Lifecycle                 | Edit, archive, restore and confirmed delete all meet expectations                      |             |
| Single active run         | While one Silo runs, the other Silos cannot start                                      |             |
| Network check             | Checks only after consent, and the result can be cleared                               |             |
| Required proxy            | Refuses to start when unavailable, does not fall back to Direct                        |             |
| Backup                    | Created successfully and refuses a silent overwrite                                    |             |
| Restore                   | A wrong passphrase and a restore while running are both refused                        |             |
| Desktop report            | Can be downloaded only after an explicit selection and confirmation                    |             |
| Companion scan            | Permission, scan and tab ownership are correct                                         |             |
| Desktop integration       | The result belongs to the current Silo, and the old binding is not reused after a stop |             |
| Light isolation           | The recommended protection can be applied and restored                                 |             |
| Labs                      | Limited coverage is shown truthfully, and the stop and receipts are correct            |             |
| Install/upgrade/uninstall | Run only when the installers are complete                                              |             |

## 19. What manual acceptance does not require you to do

The following are not your manual steps:

- Build the desktop or the extension.

- Download, install or audit dependencies.

- Run automated tests, security scans or coverage checks.

- Check signatures, hashes, the registry, lock files, process IDs or internal logs.

- Open the developer tools, the extension background inspector or any debug console.

- Start test services, the Native Host, proxy services, virtual machines or the remote Agent by hand.

- Clean build directories, dependency caches, browser test Profiles or the system temp directory.

- Use Task Manager to end all Chrome, Edge or VeriSilo processes.

If an item can continue only by doing these things, record it as "blocked" and hand it back to Codex. Your task ends there.
