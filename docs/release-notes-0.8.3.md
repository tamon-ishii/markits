# Markits Windows v0.8.3

- Added Java Access Bridge support to read UI controls in Java/Swing applications such as PyCharm. See [setup instructions](https://github.com/tamon-ishii/markits/blob/main/docs/ui-accessibility.md).
- Improved UI capture status: window frames alone no longer count as successfully detected controls.
- Commit pending crop edits before switching editor actions.

## HWiNFO and elevated applications

HWiNFO may run with administrator privileges. If Markits runs without elevation, Windows can restrict access to its UI controls. Restart Markits using **Run as administrator**, then capture again. UI control availability also depends on the application's accessibility support.