import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as vscode from 'vscode';

const execFileAsync = promisify(execFile);

interface ActiveObserver {
  label: string;
  sessionId: string;
  terminalId: string;
  workspaceRoot: string;
}

async function runSessionCli(
  workspaceRoot: string,
  command: string[],
): Promise<Record<string, unknown>> {
  const { stdout } = await execFileAsync(
    'session',
    ['--json', '--workspace', workspaceRoot, ...command],
    { cwd: workspaceRoot, windowsHide: true },
  );
  return JSON.parse(stdout) as Record<string, unknown>;
}

export function registerTerminalObserverCommand(
  context: vscode.ExtensionContext,
  outputChannel: vscode.OutputChannel,
): void {
  const activeObservers = new Map<vscode.Terminal, ActiveObserver>();
  context.subscriptions.push(
    vscode.commands.registerCommand('ticket-viewer.openSessionTerminal', async () => {
      const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
      if (!workspaceRoot) {
        void vscode.window.showWarningMessage(
          'Open a workspace before creating an observer terminal.',
        );
        return;
      }

      const sessionId = await vscode.window.showInputBox({
        title: 'Human-Owned Observer Terminal',
        prompt: 'Copilot session UUID for the observer record',
        ignoreFocusOut: true,
      });
      if (!sessionId) { return; }

      const label = await vscode.window.showInputBox({
        title: 'Human-Owned Observer Terminal',
        prompt: 'Terminal label',
        value: 'Human terminal',
        ignoreFocusOut: true,
      });
      if (!label) { return; }

      try {
        const created = await runSessionCli(workspaceRoot, [
          'terminal-create', '--session-id', sessionId,
          '--label', label, '--cwd', workspaceRoot,
        ]);
        const terminalId = created.terminal_id;
        if (typeof terminalId !== 'string') {
          throw new Error('session CLI did not return a terminal id');
        }

        const terminal = vscode.window.createTerminal({
          name: `Observer: ${label}`,
          cwd: workspaceRoot,
        });
        activeObservers.set(terminal, {
          label, sessionId, terminalId, workspaceRoot,
        });
        const closeSubscription = vscode.window.onDidCloseTerminal(closed => {
          const observer = activeObservers.get(closed);
          if (!observer) { return; }
          activeObservers.delete(closed);
          closeSubscription.dispose();
          void runSessionCli(observer.workspaceRoot, [
            'terminal-close', '--session-id', observer.sessionId,
            '--terminal-id', observer.terminalId,
          ]).catch(error => outputChannel.appendLine(
            `[ticket-viewer] observer close persistence failed: ${String(error)}`,
          ));
        });

        context.subscriptions.push(closeSubscription);
        terminal.show();
        void vscode.window.showWarningMessage(
          'Observer terminal opened. You own all input. Copy output you want to persist, then run Capture Observer Output.',
        );
      } catch (error) {
        void vscode.window.showErrorMessage(
          `Unable to create observer terminal: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }),
    vscode.commands.registerCommand('ticket-viewer.captureSessionTerminalOutput', async () => {
      const entries = [...activeObservers.entries()];
      if (entries.length === 0) {
        void vscode.window.showWarningMessage('No open observer terminal is available.');
        return;
      }
      const selected = await vscode.window.showQuickPick(
        entries.map(([, observer]) => ({
          label: observer.label,
          description: observer.terminalId,
          observer,
        })),
        { title: 'Capture Observer Output', ignoreFocusOut: true },
      );
      if (!selected) { return; }

      const output = await vscode.env.clipboard.readText();
      if (!output) {
        void vscode.window.showWarningMessage('Clipboard is empty; no observer output was persisted.');
        return;
      }
      const confirmation = await vscode.window.showWarningMessage(
        'Persist the clipboard text as terminal output? Do not capture commands, prompts, or secrets.',
        { modal: true },
        'Persist Output',
      );
      if (confirmation !== 'Persist Output') { return; }

      try {
        await runSessionCli(selected.observer.workspaceRoot, [
          'terminal-append-output', '--session-id', selected.observer.sessionId,
          '--terminal-id', selected.observer.terminalId,
          '--output', output,
        ]);
        void vscode.window.showInformationMessage('Observer output persisted.');
      } catch (error) {
        void vscode.window.showErrorMessage(
          `Unable to persist observer output: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }),
  );
}
