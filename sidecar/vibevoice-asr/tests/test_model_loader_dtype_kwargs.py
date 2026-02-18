import importlib.util
import sys
import types
import unittest
from pathlib import Path


def _load_model_loader_module():
    sidecar_dir = Path(__file__).resolve().parents[1]
    module_path = sidecar_dir / "model_loader.py"

    config_stub = types.ModuleType("config")
    config_stub.ModelConfig = object
    config_stub.PrecisionMode = str

    inference_stub = types.ModuleType("inference")

    def _noop(*_args, **_kwargs):
        return None

    for name in (
        "align_transcription_with_speakers",
        "load_audio_from_bytes",
        "normalize_transcription_segments",
        "preprocess_audio",
        "run_inference",
        "run_speaker_diarization",
    ):
        setattr(inference_stub, name, _noop)

    previous_config = sys.modules.get("config")
    previous_inference = sys.modules.get("inference")
    sys.modules["config"] = config_stub
    sys.modules["inference"] = inference_stub

    try:
        spec = importlib.util.spec_from_file_location("model_loader_under_test", module_path)
        if spec is None or spec.loader is None:
            raise RuntimeError(f"Could not load module spec for {module_path}")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        if previous_config is None:
            sys.modules.pop("config", None)
        else:
            sys.modules["config"] = previous_config
        if previous_inference is None:
            sys.modules.pop("inference", None)
        else:
            sys.modules["inference"] = previous_inference


class SanitizeNativeModelKwargsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = _load_model_loader_module()
        cls.loader = cls.module.ModelLoader

    def test_dtype_is_mapped_to_torch_dtype_and_removed(self):
        kwargs = {"dtype": "float16", "device_map": "auto"}
        sanitized = self.loader._sanitize_native_model_kwargs(kwargs)

        self.assertEqual("float16", sanitized.get("torch_dtype"))
        self.assertNotIn("dtype", sanitized)
        self.assertEqual("auto", sanitized.get("device_map"))

    def test_existing_torch_dtype_is_preserved_over_dtype(self):
        kwargs = {"dtype": "float16", "torch_dtype": "float32"}
        sanitized = self.loader._sanitize_native_model_kwargs(kwargs)

        self.assertEqual("float32", sanitized.get("torch_dtype"))
        self.assertNotIn("dtype", sanitized)

    def test_torch_dtype_remains_unchanged(self):
        kwargs = {"torch_dtype": "bfloat16", "trust_remote_code": True}
        sanitized = self.loader._sanitize_native_model_kwargs(kwargs)

        self.assertEqual("bfloat16", sanitized.get("torch_dtype"))
        self.assertTrue(sanitized.get("trust_remote_code"))
        self.assertNotIn("dtype", sanitized)

    def test_int8_related_kwargs_are_untouched(self):
        sentinel_quantization = object()
        kwargs = {
            "quantization_config": sentinel_quantization,
            "device_map": "auto",
            "torch_dtype": "float16",
        }
        sanitized = self.loader._sanitize_native_model_kwargs(kwargs)

        self.assertIs(sentinel_quantization, sanitized.get("quantization_config"))
        self.assertEqual("auto", sanitized.get("device_map"))
        self.assertEqual("float16", sanitized.get("torch_dtype"))
        self.assertNotIn("dtype", sanitized)


class TorchDTypeSerializationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = _load_model_loader_module()
        cls.loader = cls.module.ModelLoader

    def test_torch_dtype_to_serializable_with_name_attribute(self):
        class DummyDType:
            name = "float16"

        serializable = self.loader._torch_dtype_to_serializable(DummyDType())
        self.assertEqual("float16", serializable)

    def test_torch_dtype_to_serializable_with_torch_prefix(self):
        class DummyDType:
            def __str__(self):
                return "torch.float32"

        serializable = self.loader._torch_dtype_to_serializable(DummyDType())
        self.assertEqual("float32", serializable)


if __name__ == "__main__":
    unittest.main()
