# GitHub Actions Release Workflow Guide

## 🚀 Enhanced Release Workflow Features

The SK150C Kit project now includes an enhanced GitHub Actions release workflow that supports both automatic and manual version management.

## 📋 Workflow Input Parameters

### Version Mode

- **auto**: Automatically generate version based on commit messages and version type
- **manual**: Use a manually specified version number

### Manual Version

- **Format**: Semantic versioning (e.g., `1.0.0`, `2.1.3`, `0.5.0`)
- **Required**: Only when version mode is set to "manual"
- **Validation**: Must follow `major.minor.patch` format with numbers only

### Version Type (Auto Mode)

- **patch**: Increment patch version (e.g., 1.0.0 → 1.0.1)
- **minor**: Increment minor version (e.g., 1.0.0 → 1.1.0)
- **major**: Increment major version (e.g., 1.0.0 → 2.0.0)

### Prerelease

- **false**: Create a stable release
- **true**: Create a prerelease with timestamp suffix (e.g., `1.0.0-rc.20241207123456`)

## 🎯 Usage Examples

### Manual Version Release

1. Go to Actions → Release workflow
2. Click "Run workflow"
3. Set **Version Mode**: `manual`
4. Set **Manual Version**: `1.0.0`
5. Set **Prerelease**: `false`
6. Click "Run workflow"

### Automatic Version Release

1. Go to Actions → Release workflow
2. Click "Run workflow"
3. Set **Version Mode**: `auto`
4. Set **Version Type**: `minor`
5. Set **Prerelease**: `false`
6. Click "Run workflow"

## ✅ Validation Features

### Version Format Validation

- Ensures semantic versioning format (`major.minor.patch`)
- Rejects invalid formats like `1.0`, `v1.0.0`, `1.0.0-beta`

### Duplicate Tag Prevention

- Checks if the version tag already exists
- Prevents accidental duplicate releases

### Fallback Mechanisms

- If auto-generation fails, defaults to `0.1.0`
- Comprehensive error handling with clear messages

## 🔧 Generated Artifacts

Each release includes:

- **sk150c-kit** - Main firmware file (ELF format with debug info)
- **sk150c-kit.bin** - Binary firmware file for flashing
- **sk150c-kit.hex** - Intel HEX firmware file for flashing

## 📝 Changelog Generation

The workflow automatically generates changelogs:

- **First Release**: Shows "Initial release" message
- **Subsequent Releases**: Lists commits since the last valid version tag
- **Smart Tag Detection**: Filters out invalid tags and finds the last valid version

## 🛠️ Technical Details

### Tag Format

All version tags follow the format: `v{major}.{minor}.{patch}`

- Examples: `v1.0.0`, `v2.1.3`, `v0.5.0`

### Commit Message Patterns (Auto Mode)

- **Major**: Contains "BREAKING CHANGE:"
- **Minor**: Contains "feat:"
- **Patch**: All other commits

### Build Target

- **Architecture**: thumbv7em-none-eabihf
- **Microcontroller**: STM32G431CBU6
- **Toolchain**: Rust stable

## 🚨 Troubleshooting

### Common Issues

1. **"Manual version is required"**: Ensure you provide a version when using manual mode
2. **"Version does not follow semantic versioning"**: Use format like `1.0.0`, not `1.0` or `v1.0.0`
3. **"Tag already exists"**: Choose a different version number that hasn't been released

### Recovery from Issues

If you encounter problems:

1. Check the workflow logs for specific error messages
2. Ensure your version format is correct
3. Verify the tag doesn't already exist in the repository
4. Contact the maintainer if issues persist

## 📚 Best Practices

1. **Use Manual Mode** for planned releases with specific version numbers
2. **Use Auto Mode** for continuous integration and development releases
3. **Test with Prerelease** before creating stable releases
4. **Follow Semantic Versioning** principles for version numbering
5. **Review Generated Changelog** before finalizing the release
