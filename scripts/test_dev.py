"""Ensure recipe dispatch preserves argv without a platform shell."""

import unittest
from unittest.mock import patch

import dev


class DevCommands(unittest.TestCase):
    def test_just_arguments_preserve_spaces_unicode_and_shell_metacharacters(self):
        arguments = [r"C:\Users\Example User\Résumé.amigo", "$(must-not-run); & literal"]
        with patch.object(dev, "run") as run:
            dev.main(["--just", "run\n", "run", *arguments])
            self.assertEqual(run.call_args.args, ("cargo", "run", "--locked", "--package", "amigo", "--", *arguments))

    def test_native_dist_builds_once_and_only_packages_host_formats(self):
        with (patch.object(dev.dist, "native_formats", return_value=("macos", "dmg")),
              patch.object(dev.dist, "preflight"), patch.object(dev, "build") as build,
              patch.object(dev.dist, "package") as package):
            dev.dispatch("dist", [])
            build.assert_called_once()
            self.assertEqual([call.args for call in package.call_args_list], [("macos",), ("dmg",)])

    def test_foreign_package_rejection_precedes_build(self):
        with patch.object(dev.dist, "preflight", side_effect=ValueError("wrong host")), patch.object(dev, "build") as build:
            with self.assertRaisesRegex(ValueError, "wrong host"):
                dev.dispatch("package-installer", [])
            build.assert_not_called()


if __name__ == "__main__":
    unittest.main()
