import * as path from 'node:path';

describe('ticket-vscode package contributions', () => {
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