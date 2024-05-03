import asyncio

async def show_exc(foo):
    try:
        await foo
    except:
        import traceback
        traceback.print_exc()
        loop = asyncio.get_running_loop()
        loop.stop()

def run_async_tasks(fns):
    loop = asyncio.new_event_loop()
    tasks = []
    for fn in fns:
        task = loop.create_task(show_exc(fn))
        tasks.append(task)
    loop.run_forever()


