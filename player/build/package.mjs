// CI : construit le lecteur pour la plate-forme courante à partir du zip ECS
// vérifié (aucun téléchargement par l'outil), puis fixe les « fuses » d'Electron.
// Les fuses modifient le binaire : elles passent AVANT la signature VMP (EVS).
//   node package.mjs <dossier du zip ECS> <version ECS> <dossier de sortie>
import { packager } from '@electron/packager';
import { flipFuses, FuseVersion, FuseV1Options } from '@electron/fuses';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [zipDir, ecsVersion, outDir] = process.argv.slice(2);
const here = path.dirname(fileURLToPath(import.meta.url));
const appDir = path.resolve(here, '..');
const platform = process.platform === 'darwin' ? 'darwin' : 'win32';
const arch = process.platform === 'darwin' ? 'arm64' : 'x64';

const [out] = await packager({
  dir: appDir,
  out: outDir,
  overwrite: true,
  platform,
  arch,
  name: 'Lecteur NIGHTFALL',
  executableName: 'Lecteur NIGHTFALL',
  appBundleId: 'app.nightfall.lecteur',
  appCopyright: 'NIGHTFALL — MPL-2.0 ; Electron for Content Security © castLabs (MIT)',
  electronVersion: ecsVersion,
  electronZipDir: path.resolve(zipDir),
  asar: true,
  prune: false,
  ignore: [/^\/build($|\/)/],
  win32metadata: { CompanyName: 'NIGHTFALL', FileDescription: 'Lecteur NIGHTFALL', ProductName: 'Lecteur NIGHTFALL' },
});

const binary = platform === 'darwin'
  ? path.join(out, 'Lecteur NIGHTFALL.app')
  : path.join(out, 'Lecteur NIGHTFALL.exe');
await flipFuses(binary, {
  version: FuseVersion.V1,
  resetAdHocDarwinSignature: platform === 'darwin',
  [FuseV1Options.RunAsNode]: false,
  [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
  [FuseV1Options.EnableNodeCliInspectArguments]: false,
  [FuseV1Options.EnableCookieEncryption]: true,
  [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
  [FuseV1Options.OnlyLoadAppFromAsar]: true,
  [FuseV1Options.GrantFileProtocolExtraPrivileges]: false,
});
console.log('paquet : ' + out);
