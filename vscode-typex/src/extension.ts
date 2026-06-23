import * as path from 'path';
import * as fs from 'fs';
import { workspace, ExtensionContext, window } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  // find the typex_lsp binary
  const lspBinary = findLspBinary();

  if (!lspBinary) {
    window.showErrorMessage(
      'TypeX LSP binary not found. Run `cargo install --path crates/typex_lsp` to install it.',
    );
    return;
  }

  const serverOptions: ServerOptions = {
    command: lspBinary,
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'typex' }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/*.tx'),
    },
  };

  client = new LanguageClient(
    'typex_lsp',
    'TypeX Language Server',
    serverOptions,
    clientOptions,
  );

  client.start();
  console.log('TypeX LSP client started');
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

function findLspBinary(): string | undefined {
  // check ~/.cargo/bin first
  const cargoBin = path.join(
    process.env.HOME || '~',
    '.cargo',
    'bin',
    'typex_lsp',
  );
  if (fs.existsSync(cargoBin)) {
    return cargoBin;
  }

  // check PATH
  const pathDirs = (process.env.PATH || '').split(':');
  for (const dir of pathDirs) {
    const candidate = path.join(dir, 'typex_lsp');
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return undefined;
}
