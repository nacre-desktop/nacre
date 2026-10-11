import QtQuick
import QtTest

TestCase {
    id: test
    name: "PaletteTransport"
    when: windowShown
    Component {
        id: component
        NacrePaletteTask {}
    }
    function test_warm_requests_stream_then_finish_and_recover() {
        const task = createTemporaryObject(component, test, {
            command: ["python3", "/fixture/tool", "select", "/fixture/scene"]
        });
        const server = findChild(task, "paletteServer");
        const outputs = [];
        const exits = [];
        task.stdout.read.connect(text => outputs.push(JSON.parse(text)));
        task.exited.connect(code => exits.push(code));
        task.running = true;
        compare(server.writes.length, 0);
        server.stdout.read('{"ready":true}');
        compare(server.writes.length, 1);
        compare(JSON.parse(server.writes[0]).value, "/fixture/scene");
        server.stdout.read('{"presentation":{"poster":"/fixture/scene"}}');
        verify(task.running);
        compare(outputs[0].presentation.poster, "/fixture/scene");
        server.stdout.read('{"done":true,"code":0,"result":{"path":"/fixture/scene"}}');
        verify(!task.running);
        compare(exits[0], 0);
        task.running = true;
        wait(1);
        compare(server.writes.length, 2);
        server.exited(1, 0);
        verify(!task.running);
        verify(!task.ready);
        compare(exits[1], 1);
        task.running = true;
        server.stdout.read('{"ready":true}');
        compare(server.writes.length, 3);
        server.stdout.read('{"done":true,"code":1,"result":{"error":"test"}}');
        compare(exits[2], 1);
    }
}
