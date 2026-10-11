pragma Singleton
import Quickshell

Singleton {
    function run(command, workingDirectory) {
        const options = {
            command: ["systemd-run", "--user", "--scope", "--collect", "--quiet", "env", "-u", "LD_LIBRARY_PATH", "-u", "QML_IMPORT_PATH", "-u", "QML2_IMPORT_PATH", "-u", "QT_PLUGIN_PATH", "-u", "NACRE_LIB_DIR", ...command]
        };
        if (workingDirectory)
            options.workingDirectory = workingDirectory;
        Quickshell.execDetached(options);
    }
}
