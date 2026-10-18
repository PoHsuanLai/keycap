"""Pure tests for keycap-focus: the profile choice and the wire packet. Run from this directory:
python3 -m unittest. The D-Bus path is checked by hand (docs/switch-from-toshy.md)."""

import importlib.machinery
import importlib.util
import json
import os
import socket
import tempfile
import threading
import unittest

here = os.path.dirname(os.path.abspath(__file__))
loader = importlib.machinery.SourceFileLoader("keycap_focus", os.path.join(here, "keycap-focus"))
spec = importlib.util.spec_from_loader("keycap_focus", loader)
keycap_focus = importlib.util.module_from_spec(spec)
loader.exec_module(keycap_focus)


class ProfileChoice(unittest.TestCase):
    def test_terminals_get_the_terminal_profile(self):
        for name in ["konsole", "org.kde.konsole", "Konsole", "kitty", "Alacritty",
                     "org.gnome.Ptyxis", "foot", "org.wezfurlong.wezterm"]:
            self.assertEqual(keycap_focus.profile_for(name), "terminal", name)

    def test_ghostty_gets_its_own_profile_which_inherits_terminal(self):
        for name in ["ghostty", "com.mitchellh.ghostty", "Ghostty", "com.mitchellh.ghostty-debug"]:
            self.assertEqual(keycap_focus.profile_for(name), "ghostty", name)

    def test_temor_gets_terminal_on_plasma_where_kwin_takes_super(self):
        for name in ["org.quire.Temor", "org.quire.temor"]:
            self.assertEqual(keycap_focus.profile_for(name), "terminal", name)

    def test_file_managers_get_the_filemanager_profile(self):
        for name in ["org.kde.dolphin", "dolphin", "Thunar", "org.gnome.Nautilus", "nemo"]:
            self.assertEqual(keycap_focus.profile_for(name), "filemanager", name)

    def test_everything_else_gets_default(self):
        for name in ["", "firefox", "org.kde.okular", "code", "steam", "stuff", "org.kde.konsole.extra.app"]:
            self.assertEqual(keycap_focus.profile_for(name), "default", name)

    def test_the_packet_is_adjacently_tagged_json(self):
        self.assertEqual(keycap_focus.focus_message("terminal"), b'{"kind":"steer","v":{"kind":"focus","v":{"profile":"terminal"}}}')


class OnlyKWinMayReport(unittest.TestCase):
    def test_a_call_is_accepted_only_from_the_connection_that_owns_kwins_name(self):
        for sender, owner, want in [
            (":1.12", ":1.12", True),
            (":1.13", ":1.12", False),
            (":1.12", None, False),
            ("", None, False),
            (":1.120", ":1.12", False),
        ]:
            self.assertEqual(keycap_focus.from_kwin(sender, owner), want, (sender, owner))


class AgainstAFakeDaemon(unittest.TestCase):
    """A SOCK_SEQPACKET listener standing in for keycap: Hello first, then the Focus packet."""

    def listen(self, path):
        server = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
        server.bind(path)
        server.listen(1)
        return server

    def accept_one(self, server, received):
        conn, _ = server.accept()
        conn.send(b'{"kind":"hello","v":{"version":1,"status":{"mode":"running","keyboards":[]}}}')
        received.append(json.loads(conn.recv(65536)))
        conn.close()

    def test_sends_focus_and_reconnects_after_the_daemon_drops_the_connection(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "keycap.sock")
            received = []
            server = self.listen(path)
            client = keycap_focus.Keycap(path)
            for profile in ["terminal", "default"]:
                thread = threading.Thread(target=self.accept_one, args=(server, received))
                thread.start()
                self.assertTrue(client.send(profile))
                thread.join(2)
                # The fake closed the connection: the next send notices and reconnects.
            server.close()
            client._drop()
        self.assertEqual([m["v"]["profile"] for m in received], ["terminal", "default"])

    def test_restore_resends_the_profile_to_a_daemon_that_came_back(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, "keycap.sock")
            received = []
            server = self.listen(path)
            client = keycap_focus.Keycap(path)
            client.restore()  # nothing heard yet: nothing sent, nothing connected
            self.assertIsNone(client.sock)
            thread = threading.Thread(target=self.accept_one, args=(server, received))
            thread.start()
            self.assertTrue(client.send("terminal"))
            thread.join(2)
            # The fake closed the connection, as a restarting keycap does.
            thread = threading.Thread(target=self.accept_one, args=(server, received))
            thread.start()
            client.restore()
            thread.join(2)
            server.close()
            client._drop()
        self.assertEqual([m["v"]["profile"] for m in received], ["terminal", "terminal"])

    def test_an_unreachable_daemon_is_a_false_not_an_exception(self):
        client = keycap_focus.Keycap("/nonexistent/keycap.sock")
        self.assertFalse(client.send("terminal"))


if __name__ == "__main__":
    unittest.main()
