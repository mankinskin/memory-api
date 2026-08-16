import * as path from 'node:path';

describe('ticket-vscode package contributions', () => {
  test('contributes human-owned observer terminal commands', () => {
    const packageJson = require(path.resolve(__dirname, '../../package.json')) as {
      contributes: { commands: Array<{ command: string }> };
    };
    const commands = packageJson.contributes.commands.map(entry => entry.command);

    expect(commands).toContain('ticket-viewer.openSessionTerminal');
    expect(commands).toContain('ticket-viewer.captureSessionTerminalOutput');
  });

  test('Copy Ticket ID is contributed to the ticket item context menu', () => {
    const packageJson = require(path.resolve(__dirname, '../../package.json')) as {
      contributes: {
        menus: {
          'view/item/context': Array<{
            command: string;
            when?: string;
            group?: string;
          }>;
        };
      };
    };

    const copyIdEntry = packageJson.contributes.menus['view/item/context'].find(
      entry => entry.command === 'ticket-viewer.copyId' && entry.when === 'view == ticket-viewer.tickets && viewItem == ticket',
    );

    expect(copyIdEntry).toBeDefined();
    expect(copyIdEntry).toMatchObject({
      group: '0_open@2',
    });
  });
});