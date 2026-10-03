import unittest
import generate

class EligibilityTests(unittest.TestCase):
    def test_non_chat_ids_are_excluded_even_with_text_metadata(self):
        for model_id in ['gemini-embedding-2', 'text-embedding-3-small', 'rerank-v3', 'tts-1', 'gpt-image-1', 'gpt-audio', 'whisper-1', 'omni-moderation-latest']:
            with self.subTest(model_id=model_id):
                self.assertFalse(generate.eligible_model({'id': model_id, 'modalities': {'input': ['text'], 'output': ['text']}}))

    def test_non_text_output_metadata_is_excluded(self):
        for modality in ['embedding', 'rerank', 'audio', 'image', 'moderation']:
            self.assertFalse(generate.eligible_model({'id': 'unknown-specialist', 'modalities': {'input': ['text'], 'output': [modality]}}))
        self.assertTrue(generate.eligible_model({'id': 'multimodal-chat', 'modalities': {'input': ['text', 'image', 'audio'], 'output': ['text']}}))

if __name__ == '__main__':
    unittest.main()
