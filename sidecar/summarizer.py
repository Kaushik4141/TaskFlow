from basic_summarizer import BasicSummarizer
from context_builder import ContextBuilder
from models import SummarizeRequest, SummarizeResponse


class ExtractiveSummarizer:
    """
    Compatibility wrapper for older imports.
    Active pipeline: events -> ContextBuilder -> BasicSummarizer/LLMSummarizer.
    """

    def __init__(self, embedder=None):
        self.context_builder = ContextBuilder()
        self.basic_summarizer = BasicSummarizer()

    def summarize(self, request: SummarizeRequest) -> SummarizeResponse:
        context = self.context_builder.build(
            events=request.relevant_events,
            task_title=request.task_title,
            task_description=request.task_description or "",
        )
        context["event_count"] = len(request.relevant_events)
        result = self.basic_summarizer.summarize(context)
        print(f"SUMMARIZE: ExtractiveSummarizer finished using method '{result.get('method', 'basic')}'", flush=True)
        return SummarizeResponse(**result)
