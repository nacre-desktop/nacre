import ".." as Login
import QtQuick
import QtTest

TestCase {
    id: test

    function init() {
        fake.calls = 0;
        fake.received = "";
    }

    function test_login_uses_selected_session_and_blocks_duplicate_submission() {
        const view = createTemporaryObject(component, test);
        verify(view);
        wait(50);
        view.selectedSession = 1;
        view.password = "fixture-password";
        view.submit();
        view.submit();
        compare(fake.calls, 1);
        compare(fake.user, "test-user");
        compare(fake.received, "fixture-password");
        compare(fake.session, 1);
        fake.loginFailed();
        compare(view.password, "");
        compare(view.busy, false);
        verify(view.message.length > 0);
        view.password = "second-fixture";
        view.submit();
        compare(fake.calls, 2);
        fake.loginSucceeded();
        compare(view.password, "");
    }

    function test_empty_password_and_missing_session_never_authenticate() {
        const view = createTemporaryObject(component, test);
        wait(50);
        view.submit();
        compare(fake.calls, 0);
        view.selectedSession = -1;
        view.password = "fixture";
        view.submit();
        compare(fake.calls, 0);
    }

    name: "NacreLogin"
    width: 1000
    height: 700
    when: windowShown

    QtObject {
        id: fake

        property int calls: 0
        property string user: ""
        property string received: ""
        property int session: -1
        property bool canPowerOff: false
        property bool canReboot: false

        signal loginFailed
        signal loginSucceeded

        function login(user, password, index) {
            calls++;
            this.user = user;
            received = password;
            session = index;
        }
    }

    Component {
        id: component

        Login.Main {
            width: 1000
            height: 700
            backend: fake
            usersModel: [
                {
                    "name": "test-user"
                }
            ]
            sessionsModel: [
                {
                    "name": "Hyprland"
                },
                {
                    "name": "Other session"
                }
            ]
        }
    }
}
