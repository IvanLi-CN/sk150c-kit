# SK150C Shell Model Changelog

This file records detailed modification history and version changes for the SK150C kit shell models.

## Version History

### Version 1.2 - 2025 Hardware Design Improvements

#### Version 1.2 Major Improvements

**Main Body Shell (BodyMain) Modifications:**

- **2.54mm Pin Header Interface Optimization**: Improved opening dimensions for 2.54mm pin header interface to ensure better fit and connection reliability
- **Power Module Opening Enhancement**: Optimized power module opening dimensions for improved installation and thermal management
- **Front Panel PCB Mounting Fix**: Resolved conflicts in front panel interface PCB mounting positions, ensuring proper component alignment

**Back Cover Shell (BodyBackCover) Modifications:**

- **5.08mm Power Interface Optimization**: Improved opening dimensions for 5.08mm power connector interface (implemented in Rev1.1)

#### Version 1.2 Technical Details

**Interface Opening Improvements:**

- Issue Description: Previous versions had suboptimal opening dimensions that were slightly oversized
- Solution: Refined opening dimensions based on actual component measurements and installation feedback

**PCB Mounting Position Fix:**

- Issue Description: Front panel interfaces near the front face conflicted with threaded insert accommodation areas
- Solution: Raised the pocket height to eliminate conflicting sections

#### Version 1.2 File Updates

- `sk150c-shell-BodyMain_Rev1.2.step` - Main body shell containing all version 1.2 improvements
- `sk150c-shell-BodyBackCover_Rev1.2.step` - Back cover shell with improved power interface opening
- `sk150c-shell.FCStd` - FreeCAD source file updated to version 1.2

---

### Version 1.1 - 2025 Revision

#### Version 1.1 Improvements

**Main Body Shell (BodyMain) Modifications:**

- **PCB Mounting Hole Optimization**: Fixed issue where PCB mounting holes in main body shell were not fully drilled through, ensuring secure PCB installation
- **Structural Reinforcement**: Added reinforcement ribs at bottom rear section to improve overall shell strength and stability

#### Version 1.1 Technical Details

**PCB Mounting Hole Fix:**

- Issue Description: Version 1.0 had insufficient mounting hole depth, preventing screws from fully penetrating
- Solution: Redesigned mounting holes to ensure proper diameter penetration through entire wall thickness
- Impact: Significantly improved installation stability for both output PCB and input control PCB

**Bottom Rear Reinforcement Ribs:**

- Design Purpose: Enhance shell structural strength when bearing internal component weight
- Location: Bottom rear area, near back cover connection point
- Effect: Reduced shell deformation and improved overall rigidity

#### Version 1.1 File Updates

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

| Feature | Version 1.0 | Version 1.1 | Version 1.2 |
|---------|-------------|-------------|-------------|
| PCB Mounting Holes | Not fully drilled through | ✅ Fixed, fully penetrating | ✅ Maintained |
| Bottom Structure Strength | Basic design | ✅ Added rear reinforcement ribs | ✅ Maintained |
| 2.54mm Pin Header Interface | Basic opening | Unchanged | ✅ Optimized dimensions |
| Power Module Opening | Basic opening | Unchanged | ✅ Enhanced dimensions |
| Front Panel PCB Mounting | Basic design | Unchanged | ✅ Fixed position conflicts |
| 5.08mm Power Interface (Back Cover) | Basic opening | ✅ Improved dimensions | ✅ Further optimized |
| Thermal Design | Basic ventilation holes | Unchanged | Unchanged |
| Manufacturing Compatibility | ABS 3D printing validated | ABS 3D printing validated | ABS 3D printing validated |

## Recommended Version

Current Recommendation: Version 1.2

- Latest hardware design improvements
- Optimized interface opening dimensions
- Fixed PCB mounting position conflicts
- Enhanced component fit and assembly reliability
- Maintains all previous improvements from Version 1.1
- Backward compatible with existing components

## Upgrade Guide

### Upgrading from 1.1 to 1.2

If you have already manufactured shells using version 1.1:

1. **Assess Current Performance**: Check if interface components fit properly and if there are any assembly difficulties
2. **Selective Upgrade**: If current shell works without interface fitting issues, you may continue using it; for optimal fit and assembly ease, upgrade is recommended
3. **Manufacture New Version**: Use `sk150c-shell-BodyMain_Rev1.2.step` to manufacture new main body shell
4. **Back Cover Update**: Use `sk150c-shell-BodyBackCover_Rev1.2.step` for the latest back cover improvements

### Upgrading from 1.0 to 1.2

If you have shells using version 1.0:

1. **Full Upgrade Recommended**: Version 1.0 has multiple known issues that are resolved in 1.2
2. **Manufacture New Versions**: Use both `sk150c-shell-BodyMain_Rev1.2.step` and `sk150c-shell-BodyBackCover_Rev1.2.step`
3. **Significant Improvements**: You will benefit from PCB mounting fixes, structural reinforcement, and interface optimizations

### Upgrading from 1.0 to 1.1

If you have already manufactured shells using version 1.0:

1. **Assess Current Issues**: Check if PCB mounting is secure and if shell shows any deformation
2. **Selective Upgrade**: If current shell works normally, you may continue using it; for better stability, upgrade is recommended
3. **Manufacture New Version**: Use `sk150c-shell-BodyMain_Rev1.1.step` to manufacture new main body shell
4. **Back Cover Compatibility**: Existing back cover can continue to be used, no replacement needed

## Feedback and Issue Reporting

If you encounter issues during use or have improvement suggestions, please submit an Issue through the project repository.

---

Last updated: August 2025
