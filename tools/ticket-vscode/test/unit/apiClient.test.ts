import { fetchTickets } from '../../src/api';

describe('api client retry behavior', () => {
  const originalFetch = global.fetch;

  afterEach(() => {
    global.fetch = originalFetch;
    jest.restoreAllMocks();
  });

  test('retries list tickets once when the first request is aborted', async () => {
    const abortError = new Error('This operation was aborted');
    (abortError as Error & { name?: string }).name = 'AbortError';

    const okResponse = {
      ok: true,
      json: async () => ({
        request_id: 'req-1',
        workspace: 'default',
        items: [],
        next_cursor: null,
      }),
    } as unknown as Response;

    const fetchMock = jest.fn<Promise<Response>, [string, RequestInit?]>()
      .mockRejectedValueOnce(abortError)
      .mockResolvedValueOnce(okResponse);

    global.fetch = fetchMock as unknown as typeof fetch;

    const response = await fetchTickets('http://localhost:3002', 'default');

    expect(response.items).toEqual([]);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[0]?.[0]).toContain('/api/tickets?workspace=default&limit=500');
  });
});
