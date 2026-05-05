module.exports = {
  branches: ['main'],
  plugins: [
    '@semantic-release/commit-analyzer',
    '@semantic-release/release-notes-generator',
    '@semantic-release/changelog',
    ['@semantic-release/exec', {
      prepareCmd: 'scripts/bump-cargo.sh ${nextRelease.version}'
    }],
    ['@semantic-release/git', {
      assets: ['Cargo.toml', 'Cargo.lock', 'CHANGELOG.md'],
      message: 'chore(release): ${nextRelease.version} [skip ci]\n\n${nextRelease.notes}'
    }],
    ['@semantic-release/github', {
      assets: [
        { path: 'dist/vitalog-x86_64-unknown-linux-gnu/*', label: 'Linux x86_64' },
        { path: 'dist/vitalog-x86_64-apple-darwin/*',      label: 'macOS x86_64' },
        { path: 'dist/vitalog-aarch64-apple-darwin/*',     label: 'macOS ARM64' },
        { path: 'dist/vitalog-x86_64-pc-windows-msvc/*',   label: 'Windows x86_64' }
      ]
    }]
  ]
};
