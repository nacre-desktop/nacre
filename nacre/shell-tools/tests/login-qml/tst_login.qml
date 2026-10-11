import QtQuick
import QtTest

TestCase {
    id: suite
    name: "NacreLoginContracts"
    when: windowShown
    width: 1024
    height: 800
    ListModel {
        id: fixtureUsers
        property int lastIndex: 1
        ListElement {
            name: "fixture-first"
        }
        ListElement {
            name: "fixture-last"
        }
    }
    ListModel {
        id: fixtureSessions
        property int lastIndex: 1
        ListElement {
            name: "Fixture session A"
        }
        ListElement {
            name: "Fixture session B"
        }
    }
    QtObject {
        id: proxy
        property bool canReboot: false
        property bool canPowerOff: false
        property int calls: 0
        property var request: []
        signal loginFailed
        signal loginSucceeded
        function login(user, password, session) {
            calls++;
            request = [user, password, session];
        }
    }
    Component {
        id: greeter
        Main {
            width: suite.width
            height: suite.height
            usersModel: fixtureUsers
            sessionsModel: fixtureSessions
            backend: proxy
            configuration: ({
                    primary: "#47a99a",
                    secondary: "#67a499"
                })
        }
    }
    property var theme: null
    function init() {
        proxy.calls = 0;
        proxy.request = [];
        theme = createTemporaryObject(greeter, suite);
        verify(theme !== null);
        wait(0);
    }
    function test_submission_requires_credentials_models_and_backend() {
        compare(theme.selectedUser, 1);
        compare(theme.selectedSession, 1);
        theme.submit();
        compare(proxy.calls, 0);
        theme.password = "synthetic-fixture";
        theme.backend = null;
        theme.submit();
        compare(proxy.calls, 0);
        theme.backend = proxy;
        theme.selectedSession = -1;
        theme.submit();
        compare(proxy.calls, 0);
        theme.selectedSession = 1;
        theme.selectedUser = -1;
        theme.submit();
        compare(proxy.calls, 0);
    }
    function test_failure_clears_input_and_allows_explicit_retry() {
        theme.password = "synthetic-fixture";
        theme.submit();
        compare(proxy.calls, 1);
        compare(proxy.request, ["fixture-last", "synthetic-fixture", 1]);
        verify(theme.busy);
        theme.submit();
        compare(proxy.calls, 1);
        proxy.loginFailed();
        verify(!theme.busy);
        compare(theme.password, "");
        verify(theme.message.length > 0);
        theme.password = "synthetic-retry";
        theme.submit();
        compare(proxy.calls, 2);
        compare(theme.message, "");
        proxy.loginSucceeded();
        compare(theme.password, "");
    }
    function test_invalid_color_falls_back_without_changing_configuration() {
        compare(String(theme.primary), "#47a99a");
        theme.configuration = {
            primary: "invalid-color",
            secondary: "#123456"
        };
        compare(String(theme.primary), "#dbc492");
        compare(String(theme.secondary), "#123456");
        compare(theme.configuration.primary, "invalid-color");
    }
}
