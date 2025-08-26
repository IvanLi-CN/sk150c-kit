# SK150C Kit Shell Models

This directory contains 3D model files for the SK150C adjustable power supply module enclosure, designed for mechanical engineering and manufacturing.

The current models have been validated for 3D printing using ABS material.

## File Description

### Main Model Files

#### STEP Format Files (for Manufacturing)

- **`sk150c-shell-BodyMain.step`** - Main body shell STEP file
- **`sk150c-shell-BodyMain_Rev1.1.step`** - Main body shell revision 1.1 STEP file
- **`sk150c-shell-BodyBackCover.step`** - Back cover shell STEP file

#### FreeCAD Source Files

- **`sk150c-shell.FCStd`** - FreeCAD main design file

## Shell Design Features

### Main Body Shell (BodyMain)

- Houses adjustable power supply module (compatible with ZK-SK150C and similar mounting hole patterns, not compatible with XY-SK120K type)

  | SK150C                                                 | SK120X                                                 |
  | ------------------------------------------------------ | ------------------------------------------------------ |
  | ![ZK-SK150C Product Dimensions](https://ivanli.cc/api/render-image/Hardware/sk150c/assets/ZK-SK150C-%E4%BA%A7%E5%93%81%E5%B0%BA%E5%AF%B8.png?f=webp&q=85&s=1200&dpr=1) | ![XY-SK120X Product Dimensions](https://ivanli.cc/api/render-image/Hardware/sk150c/assets/XY-SK120X_%E4%BA%A7%E5%93%81%E5%B0%BA%E5%AF%B8.png?f=webp&q=85&s=1200&dpr=1) |

- Houses output PCB mounting structure and panel cutouts: USB-C, 2.54mm 2×4PIN header, DC5025
- Side passive ventilation holes for heat dissipation
- 8mm cutout for 4mm banana plugs (can be modified for 2mm banana plugs with same 8mm cutout)
- 12mm cutout for 8mm metal push button switch
- Houses input and control PCB mounting structure

### Back Cover Shell (BodyBackCover)

- Input interface cutouts: USB-C, DC5025, 5.08mm 2PIN connector
- Ventilation design with left/right exhaust and top intake, supports external 4cm fan for active intake
- Easy maintenance and assembly access

## Design Version History

### Rev 1.1 (Latest Version)

- **PCB Mounting Hole Fix**: Resolved issue where PCB mounting holes in main body were not fully drilled through
- **Structural Reinforcement**: Added reinforcement ribs at bottom rear section to improve overall strength and stability
- Maintains all original functionality and compatibility

### Rev 1.0 (Initial Version)

- Basic shell design and functionality implementation
- Known issues: PCB mounting holes not fully drilled through, insufficient structural strength at bottom rear section

📋 **For detailed change records, see: [CHANGELOG.md](./CHANGELOG.md)**

## Usage Instructions

### Viewing and Editing

1. Use **FreeCAD** to open `.FCStd` files for viewing and editing
2. Use CAD software that supports STEP format to view `.step` files

### Manufacturing Preparation

1. Recommended to use the latest **Rev 1.1** version for manufacturing ([BodyMain_Rev1.1](./models/sk150c-shell-BodyMain_Rev1.1.step), [BodyBackCover](./models/sk150c-shell-BodyBackCover.step))
2. STEP files can be used directly for 3D printing
3. Recommended materials: ABS or PETG (3D printing)

### Assembly Sequence

1. Install banana plugs and push button
2. Mount output PCB (small board, pre-solder power wires on PCB)
3. Mount input and control PCB (large board)
4. Connect banana plug, button wiring, and control wires from output PCB
5. Install adjustable power supply module (may require sanding our model shell for proper fit, designed with tight tolerances for secure mounting)
6. Connect input/output quick connectors for adjustable power supply
7. Connect remaining wiring for input and control PCB
8. Install back cover shell

## Technical Specifications

### External Dimensions

- Main body shell: To be measured
- Back cover shell: To be measured
- Overall thickness: To be measured

### Interface Cutouts

- USB-C input interface
- USB-C output interface
- Power button
- Status indicator LED

### Thermal Design

- Side ventilation holes
- Bottom heat dissipation channels
- Internal airflow paths

## Important Notes

⚠️ **Important Reminders**

- Confirm using the latest version of design files before manufacturing
- Pay attention to heat resistance of manufacturing materials
- Verify interface cutout dimensions match PCB requirements
- Ensure ventilation holes are not obstructed

## Related Files

- Project documentation: [../README.md](../README.md)
- Compatible adjustable power supply product manual: [../docs/SK150C-Digital-Power-Supply-Manual.pdf](../docs/SK150C-Digital-Power-Supply-Manual.pdf)

## Copyright Information

These design files are part of the SK150C Kit project. Please follow the project license for usage.

---
