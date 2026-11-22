# Windows UI prototype screenshots

Open the [interactive prototype](../ui-tool/index.html) to reproduce the views below.
Each PNG shows one view or interaction state. The screenshots include the
application window and omit the browser's preview controls.

| Area | PNGs | Coverage |
| --- | ---: | --- |
| [Welcome](welcome/) | 2 | Before and after the sample SMS arrives |
| [First pairing](pairing/) | 19 | QR preparation, waiting, confirmation, connection, failure, cancellation, and completion. Includes each name preset before submission, all validation errors, and the device list after saving. |
| [Add a device](add-phone/) | 16 | QR and pairing states, including completion with the same name validation cases and the updated device list after saving |
| [Main window](main/) | 37 | All device list states, expanded rows, name validation, save, scrolling, and deletion of a device that needs to pair again |
| [Settings](settings/) | 12 | All gallery settings states, including shortcut and startup errors. Also shows empty, negative, fractional, and non-numeric popup durations after the field loses focus, plus a saved duration. |
| [SMS popups](messages/) | 5 | With and without a code, long code with ellipsis, and long message at the start and end of its scroll area |
| [Tray menu](tray/) | 1 | Open Pin Glide, Settings, and Exit |
| [Historical style study](style-study/) | 66 | A/B/C directions in both palettes, every preset, and examples of settings, shortcut assignment, startup off, and closed windows |

Pairing filenames distinguish the selected input (`-input-`) from the result of
clicking Finish (`-error-`). Main window filenames identify the expanded row
(`-edit-row-`) or the error shown after clicking Save (`-edit-error-`).
The invalid-character cases exercise the prototype's Unicode validation with
an unpaired surrogate inserted into the field.
