# SK150C Shell Model Changelog

This file records detailed modification history and version changes for the SK150C kit shell models.

## Version History

### Version 1.1 - 2025 Revision

#### Major Improvements

**Main Body Shell (BodyMain) Modifications:**
- **PCB Mounting Hole Optimization**: Fixed issue where PCB mounting holes in main body shell were not fully drilled through, ensuring secure PCB installation
- **Structural Reinforcement**: Added reinforcement ribs at bottom rear section to improve overall shell strength and stability

#### Technical Details

**PCB Mounting Hole Fix:**
- Issue Description: Version 1.0 had insufficient mounting hole depth, preventing screws from fully penetrating
- Solution: Redesigned mounting holes to ensure proper diameter penetration through entire wall thickness
- Impact: Significantly improved installation stability for both output PCB and input control PCB

**Bottom Rear Reinforcement Ribs:**
- Design Purpose: Enhance shell structural strength when bearing internal component weight
- Location: Bottom rear area, near back cover connection point
- Effect: Reduced shell deformation and improved overall rigidity

#### File Updates

- `sk150c-shell-BodyMain_Rev1.1.step` - Main body shell containing all version 1.1 improvements
- `sk150c-shell.FCStd` - FreeCAD source file updated to version 1.1

---

### Version 1.0 - Initial Release

#### Initial Features

**Main Body Shell (BodyMain) Characteristics:**
- Basic structure for housing ZK-SK150C adjustable power supply module
- Output PCB mounting positions and panel cutout design
- Side passive ventilation hole design
- 8mm banana plug cutouts (4mm plug compatible)
- 12mm metal push button switch cutout
- Input and control PCB housing space

**Back Cover Shell (BodyBackCover) Characteristics:**
- Input interface cutouts: USB-C, DC5025, 5.08mm 2PIN
- Ventilation design: left/right exhaust, top intake
- Support for 4cm fan external intake mounting
- Basic maintenance and assembly accessibility

#### Known Issues (Fixed in Version 1.1)

- PCB mounting holes not fully drilled through, affecting installation stability
- Insufficient structural strength at bottom rear section, potential for minor deformation

#### File List

- `sk150c-shell-BodyMain.step` - Version 1.0 main body shell
- `sk150c-shell-BodyBackCover.step` - Back cover shell (compatible with both 1.0 and 1.1)

---

## Version Comparison

| Feature | Version 1.0 | Version 1.1 |
|---------|-------------|-------------|
| PCB Mounting Holes | Not fully drilled through | ✅ Fixed, fully penetrating |
| Bottom Structure Strength | Basic design | ✅ Added rear reinforcement ribs |
| Thermal Design | Basic ventilation holes | Unchanged |
| Interface Cutouts | Complete design | Unchanged |
| Manufacturing Compatibility | ABS 3D printing validated | ABS 3D printing validated |

## Recommended Version

**Current Recommendation: Version 1.1**
- Fixes PCB installation issues
- Enhanced structural strength
- Maintains all original functionality
- Backward compatible with existing components

## Upgrade Guide

### Upgrading from 1.0 to 1.1

If you have already manufactured shells using version 1.0:

1. **Assess Current Issues**: Check if PCB mounting is secure and if shell shows any deformation
2. **Selective Upgrade**: If current shell works normally, you may continue using it; for better stability, upgrade is recommended
3. **Manufacture New Version**: Use `sk150c-shell-BodyMain_Rev1.1.step` to manufacture new main body shell
4. **Back Cover Compatibility**: Existing back cover can continue to be used, no replacement needed

## Feedback and Issue Reporting

If you encounter issues during use or have improvement suggestions, please submit an Issue through the project repository.

---

*Last updated: August 2025*