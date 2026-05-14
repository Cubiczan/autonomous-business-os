from app.agents.knowledge_communication import KnowledgeCommunicationAgent


def test_meeting_summary_extracts_action_items() -> None:
    agent = object.__new__(KnowledgeCommunicationAgent)

    result = agent._summarize_meeting(
        "We reviewed the launch plan. Action: Sam will confirm owners. "
        "Todo: Priya will update the client timeline. Budget is unchanged."
    )

    assert "launch plan" in result["summary"]
    assert len(result["action_items"]) == 2
